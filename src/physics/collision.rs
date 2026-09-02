use glam::{IVec3, Vec3};
use std::collections::HashMap;

use super::body::DynamicBody;
use crate::streaming::store::ChunkStore;
use crate::voxel::VOXEL_SIZE;

/// Peta ketinggian voxel relatif per kolom horizontal (X, Z)
pub type ColumnExtents = HashMap<(i32, i32), i32>;

/// Hasil pengujian tabrakan vertikal sejati (Swept Vertical Collision)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VerticalCollisionResult {
    /// Tidak ada kontak, lintasan bebas hingga posisi target
    Clear { target_pos: Vec3 },
    /// Kontak dengan permukaan solid di bawahnya (tanah)
    GroundContact {
        clamped_pos: Vec3,
        contact_voxel_y: i32,
    },
    /// Kontak dengan permukaan solid di atasnya (langit-langit)
    CeilingContact {
        clamped_pos: Vec3,
        contact_voxel_y: i32,
    },
    /// Tertahan oleh batas chunk yang belum dimuat (Unloaded / Unknown)
    BlockedByUnloaded { clamped_pos: Vec3 },
}

/// Menghitung kolom horizontal dan voxel terbawah/teratas untuk setiap kolom (X, Z) dari suatu aggregate
pub fn get_aggregate_vertical_extents(body: &DynamicBody) -> (ColumnExtents, ColumnExtents) {
    let mut min_y_per_col: HashMap<(i32, i32), i32> = HashMap::new();
    let mut max_y_per_col: HashMap<(i32, i32), i32> = HashMap::new();

    for v in &body.aggregate.voxels {
        let col = (v.relative_coord.x, v.relative_coord.z);
        let y = v.relative_coord.y;

        min_y_per_col
            .entry(col)
            .and_modify(|cur| *cur = (*cur).min(y))
            .or_insert(y);

        max_y_per_col
            .entry(col)
            .and_modify(|cur| *cur = (*cur).max(y))
            .or_insert(y);
    }

    (min_y_per_col, max_y_per_col)
}

/// Memeriksa apakah dasar dari aggregate bertumpu stabil pada setidaknya satu voxel solid statis di bawahnya
pub fn is_firmly_supported_by_static_ground(body: &DynamicBody, store: &ChunkStore) -> bool {
    let (bottom_cols, _) = get_aggregate_vertical_extents(body);
    let base_voxel = body.current_base_voxel();

    for (&(local_x, local_z), &local_min_y) in &bottom_cols {
        let world_x = base_voxel.x + local_x;
        let world_z = base_voxel.z + local_z;
        let ground_y = base_voxel.y + local_min_y - 1;

        if let Some(block) = store.get_voxel_world_checked(IVec3::new(world_x, ground_y, world_z)) {
            if !block.is_air() {
                return true;
            }
        }
    }
    false
}

/// Mengevaluasi transisi status Sleeping dan Settled (Amendment 13).
///
/// INVARIANTS:
/// - Sleeping: Kecepatan di bawah ambang batas selama >= sleep_ticks_required.
/// - Settled: Harus memiliki gravity_scale > 0.0, is_grounded == true, dan bertumpu pada tanah solid.
/// - AntiGravity (gravity_scale == 0.0) TIDAK PERNAH SETTLED (tetap dinamis).
pub fn update_body_sleep_and_settle(
    body: &mut DynamicBody,
    config: &super::config::PhysicsConfig,
    store: &ChunkStore,
) {
    let speed = body.velocity.length();

    if speed < config.sleep_velocity_threshold {
        body.ticks_stationary = body.ticks_stationary.saturating_add(1);
    } else {
        body.ticks_stationary = 0;
        if body.state == super::body::DynamicBodyState::Sleeping {
            body.set_state(super::body::DynamicBodyState::Active);
        }
        return;
    }

    if body.ticks_stationary >= config.sleep_ticks_required {
        if body.gravity_scale > 0.0
            && body.is_grounded
            && is_firmly_supported_by_static_ground(body, store)
        {
            body.set_state(super::body::DynamicBodyState::Settled);
        } else {
            body.set_state(super::body::DynamicBodyState::Sleeping);
        }
    }
}

/// Menguji tabrakan swept vertikal sepanjang interval translasi satu tick fisika.
///
/// INVARIANTS:
/// - Menguji seluruh interval vertikal dari posisi awal ke posisi kandidat (Amendment 4).
/// - Unloaded chunk diidentifikasi sebagai Unknown, BUKAN Air (Amendment 6).
/// - Saat terjadi kontak dengan tanah, posisi di-snap ke batas kisi integer voxel (Amendment 9 & 10).
pub fn swept_vertical_step(
    body: &DynamicBody,
    cand_delta_y: f32,
    store: &ChunkStore,
) -> VerticalCollisionResult {
    if cand_delta_y == 0.0 {
        return VerticalCollisionResult::Clear {
            target_pos: body.position,
        };
    }

    let (bottom_cols, top_cols) = get_aggregate_vertical_extents(body);
    let base_voxel = body.current_base_voxel();

    if cand_delta_y < 0.0 {
        // ====================================================================
        // JATUH KE BAWAH (GRAVITASI NORMAL)
        // ====================================================================
        let start_pos_y = body.position.y;
        let cand_pos_y = start_pos_y + cand_delta_y;

        let start_base_voxel_y = base_voxel.y;
        let cand_base_voxel_y = (cand_pos_y / VOXEL_SIZE).floor() as i32;

        let mut earliest_contact_pos_y: Option<f32> = None;
        let mut highest_contact_voxel_y: Option<i32> = None;
        let mut blocked_by_unloaded = false;

        // Periksa setiap kolom horizontal aggregate
        for (&(local_x, local_z), &local_min_y) in &bottom_cols {
            let world_x = base_voxel.x + local_x;
            let world_z = base_voxel.z + local_z;

            let start_test_y = start_base_voxel_y + local_min_y;
            let end_test_y = cand_base_voxel_y + local_min_y;

            // Swept dari voxel tepat di bawah posisi awal hingga voxel kandidat terendah
            for test_y in (end_test_y..=start_test_y).rev() {
                let test_coord = IVec3::new(world_x, test_y, world_z);

                match store.get_voxel_world_checked(test_coord) {
                    None => {
                        // Chunk belum dimuat: UNKNOWN != AIR (Amendment 6)
                        // Badan tidak boleh jatuh menembus chunk yang belum ada
                        blocked_by_unloaded = true;
                        let safe_rest_y = (test_y + 1 - local_min_y) as f32 * VOXEL_SIZE;
                        earliest_contact_pos_y = Some(match earliest_contact_pos_y {
                            Some(prev) => prev.max(safe_rest_y),
                            None => safe_rest_y,
                        });
                        break;
                    }
                    Some(block) => {
                        if !block.is_air() {
                            // Kontak solid terdeteksi!
                            // Voxel terbawah kolom ini harus bertumpu tepat di test_y + 1 (di atas balok solid)
                            let contact_voxel_y = test_y;
                            let snap_rest_y =
                                (contact_voxel_y + 1 - local_min_y) as f32 * VOXEL_SIZE;

                            earliest_contact_pos_y = Some(match earliest_contact_pos_y {
                                Some(prev) => prev.max(snap_rest_y),
                                None => snap_rest_y,
                            });
                            highest_contact_voxel_y = Some(match highest_contact_voxel_y {
                                Some(prev) => prev.max(contact_voxel_y),
                                None => contact_voxel_y,
                            });
                            break;
                        }
                    }
                }
            }
        }

        if let Some(rest_y) = earliest_contact_pos_y {
            // Clamping posisi dengan snap deterministik
            let clamped_pos = Vec3::new(body.position.x, rest_y.max(cand_pos_y), body.position.z);
            if blocked_by_unloaded {
                VerticalCollisionResult::BlockedByUnloaded { clamped_pos }
            } else {
                VerticalCollisionResult::GroundContact {
                    clamped_pos,
                    contact_voxel_y: highest_contact_voxel_y.unwrap_or(base_voxel.y),
                }
            }
        } else {
            VerticalCollisionResult::Clear {
                target_pos: Vec3::new(body.position.x, cand_pos_y, body.position.z),
            }
        }
    } else {
        // ====================================================================
        // NAIK KE ATAS (GRAVITASI TERBALIK)
        // ====================================================================
        let start_pos_y = body.position.y;
        let cand_pos_y = start_pos_y + cand_delta_y;

        let start_base_voxel_y = base_voxel.y;
        let cand_base_voxel_y = (cand_pos_y / VOXEL_SIZE).floor() as i32;

        let mut earliest_ceiling_pos_y: Option<f32> = None;
        let mut lowest_contact_voxel_y: Option<i32> = None;
        let mut blocked_by_unloaded = false;

        for (&(local_x, local_z), &local_max_y) in &top_cols {
            let world_x = base_voxel.x + local_x;
            let world_z = base_voxel.z + local_z;

            let start_test_y = start_base_voxel_y + local_max_y;
            let end_test_y = cand_base_voxel_y + local_max_y;

            for test_y in start_test_y..=end_test_y {
                let test_coord = IVec3::new(world_x, test_y, world_z);

                match store.get_voxel_world_checked(test_coord) {
                    None => {
                        blocked_by_unloaded = true;
                        let safe_rest_y = (test_y - 1 - local_max_y) as f32 * VOXEL_SIZE;
                        earliest_ceiling_pos_y = Some(match earliest_ceiling_pos_y {
                            Some(prev) => prev.min(safe_rest_y),
                            None => safe_rest_y,
                        });
                        break;
                    }
                    Some(block) => {
                        if !block.is_air() {
                            let contact_voxel_y = test_y;
                            let snap_rest_y =
                                (contact_voxel_y - 1 - local_max_y) as f32 * VOXEL_SIZE;

                            earliest_ceiling_pos_y = Some(match earliest_ceiling_pos_y {
                                Some(prev) => prev.min(snap_rest_y),
                                None => snap_rest_y,
                            });
                            lowest_contact_voxel_y = Some(match lowest_contact_voxel_y {
                                Some(prev) => prev.min(contact_voxel_y),
                                None => contact_voxel_y,
                            });
                            break;
                        }
                    }
                }
            }
        }

        if let Some(rest_y) = earliest_ceiling_pos_y {
            let clamped_pos = Vec3::new(body.position.x, rest_y.min(cand_pos_y), body.position.z);
            if blocked_by_unloaded {
                VerticalCollisionResult::BlockedByUnloaded { clamped_pos }
            } else {
                VerticalCollisionResult::CeilingContact {
                    clamped_pos,
                    contact_voxel_y: lowest_contact_voxel_y.unwrap_or(base_voxel.y),
                }
            }
        } else {
            VerticalCollisionResult::Clear {
                target_pos: Vec3::new(body.position.x, cand_pos_y, body.position.z),
            }
        }
    }
}
