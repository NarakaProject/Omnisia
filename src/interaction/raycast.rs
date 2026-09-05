use glam::Vec3;

use super::types::{VoxelHit, VoxelRaycastResult};
use crate::coord::world_pos_to_world_voxel;
use crate::mesh::types::FaceDirection;
use crate::player::PlayerController;
use crate::streaming::store::ChunkStore;
use crate::voxel::VOXEL_SIZE;

/// Melakukan raycast 3D DDA deterministik terhadap ChunkStore (Phase 11.1).
///
/// Karakteristik & Invariant:
/// - Murni read-only: tidak memutasi ChunkStore, tidak memicu generasi chunk atau disk I/O.
/// - Menghormati residency: jika menembus chunk yang belum termuat, langsung mengembalikan `NonResident`.
/// - Menghitung titik potong exact pada bidang permukaan voxel dan normal kanonikal 6-arah (+X, -X, +Y, -Y, +Z, -Z).
/// - Mendukung koordinat negatif secara kontinu menggunakan Euclidean division (`div_euclid`).
/// - Zero heap allocations: traversal berjalan secara $O(\text{reach} / \text{voxel\_size})$ di stack.
pub fn raycast_voxels(
    store: &ChunkStore,
    origin: Vec3,
    direction: Vec3,
    max_reach: f32,
) -> VoxelRaycastResult {
    // 1. Validasi input awal
    if max_reach <= 0.0 || origin.is_nan() || direction.is_nan() {
        return VoxelRaycastResult::Miss;
    }

    let dir_len_sq = direction.length_squared();
    if dir_len_sq < 1e-8 {
        return VoxelRaycastResult::Miss;
    }

    let dir = direction.normalize();

    // 2. Evaluasi voxel awal di mana origin berada
    let mut current_voxel = world_pos_to_world_voxel(origin);

    match store.get_voxel_world_checked(current_voxel) {
        None => {
            // Origin berada di dalam chunk yang tidak resident
            return VoxelRaycastResult::NonResident {
                voxel_coord: current_voxel,
                distance: 0.0,
                hit_point: origin,
                face: FaceDirection::PosY,
            };
        }
        Some(block) => {
            if block.is_solid() {
                // Origin sudah berada di dalam voxel solid sejak t = 0.0
                let face = if dir.x.abs() >= dir.y.abs() && dir.x.abs() >= dir.z.abs() {
                    if dir.x > 0.0 {
                        FaceDirection::NegX
                    } else {
                        FaceDirection::PosX
                    }
                } else if dir.y.abs() >= dir.z.abs() {
                    if dir.y > 0.0 {
                        FaceDirection::NegY
                    } else {
                        FaceDirection::PosY
                    }
                } else if dir.z > 0.0 {
                    FaceDirection::NegZ
                } else {
                    FaceDirection::PosZ
                };

                return VoxelRaycastResult::Hit(VoxelHit {
                    voxel_coord: current_voxel,
                    material: block.material,
                    hit_point: origin,
                    distance: 0.0,
                    face,
                    normal: face.normal_vec3(),
                });
            }
        }
    }

    // 3. Inisialisasi parameter traversal Amanatides-Woo 3D DDA
    let (step_x, mut t_max_x, t_delta_x) = if dir.x > 1e-8 {
        let next_boundary = (current_voxel.x + 1) as f32 * VOXEL_SIZE;
        let t = (next_boundary - origin.x) / dir.x;
        (1, t.max(0.0), VOXEL_SIZE / dir.x)
    } else if dir.x < -1e-8 {
        let next_boundary = current_voxel.x as f32 * VOXEL_SIZE;
        let t = (next_boundary - origin.x) / dir.x;
        (-1, t.max(0.0), VOXEL_SIZE / -dir.x)
    } else {
        (0, f32::INFINITY, f32::INFINITY)
    };

    let (step_y, mut t_max_y, t_delta_y) = if dir.y > 1e-8 {
        let next_boundary = (current_voxel.y + 1) as f32 * VOXEL_SIZE;
        let t = (next_boundary - origin.y) / dir.y;
        (1, t.max(0.0), VOXEL_SIZE / dir.y)
    } else if dir.y < -1e-8 {
        let next_boundary = current_voxel.y as f32 * VOXEL_SIZE;
        let t = (next_boundary - origin.y) / dir.y;
        (-1, t.max(0.0), VOXEL_SIZE / -dir.y)
    } else {
        (0, f32::INFINITY, f32::INFINITY)
    };

    let (step_z, mut t_max_z, t_delta_z) = if dir.z > 1e-8 {
        let next_boundary = (current_voxel.z + 1) as f32 * VOXEL_SIZE;
        let t = (next_boundary - origin.z) / dir.z;
        (1, t.max(0.0), VOXEL_SIZE / dir.z)
    } else if dir.z < -1e-8 {
        let next_boundary = current_voxel.z as f32 * VOXEL_SIZE;
        let t = (next_boundary - origin.z) / dir.z;
        (-1, t.max(0.0), VOXEL_SIZE / -dir.z)
    } else {
        (0, f32::INFINITY, f32::INFINITY)
    };

    // Batas pengaman jumlah iterasi untuk mencegah infinite loop pada kasus float abnormal
    let max_steps = ((max_reach / VOXEL_SIZE).ceil() as usize + 2) * 3;

    // 4. Loop DDA: melangkah dari satu batas voxel ke batas berikutnya
    for _ in 0..max_steps {
        let (t_hit, face);
        if t_max_x < t_max_y {
            if t_max_x < t_max_z {
                t_hit = t_max_x;
                face = if step_x > 0 {
                    FaceDirection::NegX
                } else {
                    FaceDirection::PosX
                };
                current_voxel.x += step_x;
                t_max_x += t_delta_x;
            } else {
                t_hit = t_max_z;
                face = if step_z > 0 {
                    FaceDirection::NegZ
                } else {
                    FaceDirection::PosZ
                };
                current_voxel.z += step_z;
                t_max_z += t_delta_z;
            }
        } else if t_max_y < t_max_z {
            t_hit = t_max_y;
            face = if step_y > 0 {
                FaceDirection::NegY
            } else {
                FaceDirection::PosY
            };
            current_voxel.y += step_y;
            t_max_y += t_delta_y;
        } else {
            t_hit = t_max_z;
            face = if step_z > 0 {
                FaceDirection::NegZ
            } else {
                FaceDirection::PosZ
            };
            current_voxel.z += step_z;
            t_max_z += t_delta_z;
        }

        // Aturan Jangkauan: batas reach bersifat inklusif (t_hit <= max_reach).
        // Jika jarak kontak melampaui max_reach, ray dinyatakan meleset (Miss).
        if t_hit > max_reach {
            return VoxelRaycastResult::Miss;
        }

        // Menghitung titik potong geometris pada bidang sisi voxel
        let mut hit_point = origin + dir * t_hit;
        match face {
            FaceDirection::NegX => hit_point.x = current_voxel.x as f32 * VOXEL_SIZE,
            FaceDirection::PosX => hit_point.x = (current_voxel.x + 1) as f32 * VOXEL_SIZE,
            FaceDirection::NegY => hit_point.y = current_voxel.y as f32 * VOXEL_SIZE,
            FaceDirection::PosY => hit_point.y = (current_voxel.y + 1) as f32 * VOXEL_SIZE,
            FaceDirection::NegZ => hit_point.z = current_voxel.z as f32 * VOXEL_SIZE,
            FaceDirection::PosZ => hit_point.z = (current_voxel.z + 1) as f32 * VOXEL_SIZE,
        }

        // Kueri voxel otoritatif dari ChunkStore
        match store.get_voxel_world_checked(current_voxel) {
            None => {
                // Ray memasuki ruang chunk yang belum dimuat (non-resident)
                return VoxelRaycastResult::NonResident {
                    voxel_coord: current_voxel,
                    distance: t_hit,
                    hit_point,
                    face,
                };
            }
            Some(block) => {
                if block.is_solid() {
                    return VoxelRaycastResult::Hit(VoxelHit {
                        voxel_coord: current_voxel,
                        material: block.material,
                        hit_point,
                        distance: t_hit,
                        face,
                        normal: face.normal_vec3(),
                    });
                }
            }
        }
    }

    VoxelRaycastResult::Miss
}

/// Melakukan query interaksi voxel dari sudut pandang mata pemain (eye position)
/// menggunakan jangkauan interaksi default yang tercatat pada PlayerConfig.
#[inline(always)]
pub fn raycast_player_interaction(
    store: &ChunkStore,
    player: &PlayerController,
    look_direction: Vec3,
) -> VoxelRaycastResult {
    raycast_voxels(
        store,
        player.eye_position(),
        look_direction,
        player.config.interaction_reach,
    )
}

/// Melakukan query interaksi voxel dari sudut pandang mata pemain dengan jangkauan eksplisit.
#[inline(always)]
pub fn raycast_player_interaction_with_reach(
    store: &ChunkStore,
    player: &PlayerController,
    look_direction: Vec3,
    max_reach: f32,
) -> VoxelRaycastResult {
    raycast_voxels(store, player.eye_position(), look_direction, max_reach)
}
