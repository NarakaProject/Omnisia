use glam::{IVec3, Vec3};

use crate::physics::PhysicsRuntime;
use crate::streaming::store::ChunkStore;
use crate::voxel::VOXEL_SIZE;

pub const GROUND_CONTACT_EPSILON: f32 = 0.05;

/// Toleransi penetrasi ke bawah permukaan tumpuan (meter).
///
/// INVARIAN (Phase 8D.4):
/// Diselaraskan simetris dengan epsilon tumpuan (0.05m).
/// Mencegah false airborne ketika pemain bergerak dari tumpuan tepi (di mana kaki berada pada
/// ~0.96m akibat geometri bola) menuju permukaan datar (1.00m).
pub const GROUND_PENETRATION_TOLERANCE: f32 = 0.05;

/// Hasil evaluasi kontak tumpuan tanah untuk kapsul pemain (8B.2 & 8D.4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundContactResult {
    /// Apakah pemain bertumpu stabil pada permukaan tanah
    pub grounded: bool,
    /// Normal permukaan tumpuan (kanonikal +Y)
    pub ground_normal: Vec3,
    /// Jarak vertikal dari kurva kapsul bawah ke permukaan tanah (>= 0.0)
    pub ground_distance: f32,
    /// Koordinat voxel dunia yang memberikan tumpuan
    pub support_voxel: Option<IVec3>,
    /// Ketinggian Y permukaan atas tumpuan dalam meter (surface_y)
    pub ground_y_surface: Option<f32>,
    /// Ketinggian Y posisi telapak kaki (PlayerState.position.y / capsule base) yang stabil secara geometris (Phase 8D.4)
    pub stable_feet_y: Option<f32>,
}

impl Default for GroundContactResult {
    fn default() -> Self {
        Self {
            grounded: false,
            ground_normal: Vec3::Y,
            ground_distance: f32::INFINITY,
            support_voxel: None,
            ground_y_surface: None,
            stable_feet_y: None,
        }
    }
}

/// Memeriksa apakah telapak kaki pemain bertumpu pada permukaan tanah solid (8B.2 & 8D.4).
///
/// ATURAN KANONIKAL & GEOMETRI (Phase 8D.4):
/// - 1 voxel = 0.5 meter (VOXEL_SIZE).
/// - Permukaan atas voxel Y berada di $y_{\text{surface}} = (Y + 1) \times 0.5\text{m}$.
/// - Model Footprint-Aware Lower-Hemisphere:
///   Mengevaluasi setiap permukaan atas voxel solid yang terekspos (`Known Solid` di bawah, `Known Air` di atas)
///   dalam jangkauan horizontal footprint lingkaran kapsul ($feet\_pos.xz \pm radius$) dan rentang vertikal kontak.
/// - Menghitung jarak geometris aktual antara belahan bola bawah kapsul pada offset horizontal $d \le radius$
///   dan permukaan atas voxel:
///   $y_{\text{capsule\_bottom}} = (feet\_pos.y + radius) - \sqrt{radius^2 - d^2}$.
///   $vertical\_dist = y_{\text{capsule\_bottom}} - y_{\text{surface}}$.
/// - Kontak valid jika $-penetration\_tol \le vertical\_dist \le epsilon$ (dengan $penetration\_tol = 0.03\text{m}$).
/// - Ketinggian telapak kaki stabil yang presisi:
///   $stable\_feet\_y = y_{\text{surface}} - (radius - \sqrt{radius^2 - d^2})$.
/// - Seleksi kandidat terbaik secara deterministik (Amendment 7): error kontak absolut terkecil,
///   lalu jarak horizontal terdekat, lalu tie-break koordinat deterministik.
/// - Hard Invariant Unknown != Air (Amendment 4): jika voxel tumpuan atau voxel di atasnya Unknown,
///   kandidat ditolak dan tidak pernah menghasilkan `grounded = true`.
/// - Bebas alokasi heap (Zero Allocation).
pub fn check_ground_support(
    feet_pos: Vec3,
    radius: f32,
    epsilon: f32,
    store: &ChunkStore,
) -> GroundContactResult {
    let penetration_tol = GROUND_PENETRATION_TOLERANCE;

    // 1. Rentang horizontal kotak voxel yang bersinggungan dengan footprint lingkaran kaki (radius)
    let vx_min = ((feet_pos.x - radius) / VOXEL_SIZE).floor() as i32;
    let vx_max = ((feet_pos.x + radius) / VOXEL_SIZE).floor() as i32;
    let vz_min = ((feet_pos.z - radius) / VOXEL_SIZE).floor() as i32;
    let vz_max = ((feet_pos.z + radius) / VOXEL_SIZE).floor() as i32;

    // 2. Rentang vertikal permukaan atas voxel yang mungkin bersentuhan dengan belahan bola bawah kapsul:
    // Permukaan atas minimum: feet_pos.y - epsilon
    // Permukaan atas maksimum: feet_pos.y + radius + penetration_tol
    let min_surface_y = feet_pos.y - epsilon;
    let max_surface_y = feet_pos.y + radius + penetration_tol;

    let vy_min = ((min_surface_y / VOXEL_SIZE).floor() as i32) - 1;
    let vy_max = ((max_surface_y / VOXEL_SIZE).ceil() as i32) - 1;

    struct Candidate {
        coord: IVec3,
        surface_y: f32,
        stable_feet_y: f32,
        vertical_dist: f32,
        abs_vertical_dist: f32,
        horiz_dist_sq: f32,
    }

    let mut best_candidate: Option<Candidate> = None;

    // 3. Evaluasi setiap kandidat voxel dalam rentang kontak 3D
    for vy in vy_min..=vy_max {
        let surface_y = (vy + 1) as f32 * VOXEL_SIZE;
        if surface_y < min_surface_y || surface_y > max_surface_y {
            continue;
        }

        for vz in vz_min..=vz_max {
            for vx in vx_min..=vx_max {
                let coord = IVec3::new(vx, vy, vz);
                let above_coord = IVec3::new(vx, vy + 1, vz);

                // HARD INVARIANT (Amendment 2 & 4):
                // Voxel kandidat harus Known Solid (Some && !is_air), dan
                // Voxel tepat di atasnya harus Known Air (Some && is_air).
                // Unknown (None) BUKAN Air dan TIDAK PERNAH menghasilkan tumpuan valid!
                let current_block = store.get_voxel_world_checked(coord);
                let above_block = store.get_voxel_world_checked(above_coord);

                let is_exposed_top = match (current_block, above_block) {
                    (Some(curr), Some(above)) => !curr.is_air() && above.is_air(),
                    _ => false,
                };

                if !is_exposed_top {
                    continue;
                }

                // Titik terdekat pada permukaan atas voxel ke sumbu vertikal kapsul
                let box_min_x = vx as f32 * VOXEL_SIZE;
                let box_max_x = box_min_x + VOXEL_SIZE;
                let box_min_z = vz as f32 * VOXEL_SIZE;
                let box_max_z = box_min_z + VOXEL_SIZE;

                let closest_x = feet_pos.x.clamp(box_min_x, box_max_x);
                let closest_z = feet_pos.z.clamp(box_min_z, box_max_z);
                let dx = feet_pos.x - closest_x;
                let dz = feet_pos.z - closest_z;
                let horiz_dist_sq = dx * dx + dz * dz;

                if horiz_dist_sq > (radius * radius) {
                    continue;
                }

                // Geometri belahan bola bawah kapsul pada offset horizontal terdekat
                let y_offset = (radius * radius - horiz_dist_sq).max(0.0).sqrt();
                let capsule_bottom_at_contact = (feet_pos.y + radius) - y_offset;
                let vertical_dist = capsule_bottom_at_contact - surface_y;

                // Uji toleransi kontak terpisah:
                // -penetration_tol <= vertical_dist <= epsilon
                if vertical_dist < -penetration_tol || vertical_dist > epsilon {
                    continue;
                }

                // Ketinggian kaki stabil: posisi base kapsul jika menempel tepat pada permukaan ini
                let stable_feet_y = surface_y - (radius - y_offset);
                let abs_vertical_dist = vertical_dist.abs();

                let candidate = Candidate {
                    coord,
                    surface_y,
                    stable_feet_y,
                    vertical_dist,
                    abs_vertical_dist,
                    horiz_dist_sq,
                };

                // Seleksi kandidat terbaik secara deterministik (Amendment 7):
                // 1. Error kontak absolut terkecil
                // 2. Jarak horizontal terkecil ke pusat kaki
                // 3. Tie-break koordinat deterministik
                let is_better = match &best_candidate {
                    None => true,
                    Some(best) => {
                        let diff = candidate.abs_vertical_dist - best.abs_vertical_dist;
                        if diff < -1e-5 {
                            true
                        } else if diff > 1e-5 {
                            false
                        } else {
                            let horiz_diff = candidate.horiz_dist_sq - best.horiz_dist_sq;
                            if horiz_diff < -1e-5 {
                                true
                            } else if horiz_diff > 1e-5 {
                                false
                            } else {
                                (candidate.coord.y, -candidate.coord.x, -candidate.coord.z)
                                    > (best.coord.y, -best.coord.x, -best.coord.z)
                            }
                        }
                    }
                };

                if is_better {
                    best_candidate = Some(candidate);
                }
            }
        }
    }

    if let Some(best) = best_candidate {
        GroundContactResult {
            grounded: true,
            ground_normal: Vec3::Y,
            ground_distance: best.vertical_dist.max(0.0),
            support_voxel: Some(best.coord),
            ground_y_surface: Some(best.surface_y),
            stable_feet_y: Some(best.stable_feet_y),
        }
    } else {
        GroundContactResult::default()
    }
}

/// Evaluasi kontak tumpuan tanah dengan dukungan PhysicsRuntime DynamicBody (8C.2 & 8D.4).
pub fn check_ground_support_with_physics(
    feet_pos: Vec3,
    radius: f32,
    epsilon: f32,
    store: &ChunkStore,
    physics: Option<&PhysicsRuntime>,
) -> GroundContactResult {
    let static_res = check_ground_support(feet_pos, radius, epsilon, store);
    if static_res.grounded {
        return static_res;
    }

    let penetration_tol = GROUND_PENETRATION_TOLERANCE;
    let min_surface_y = feet_pos.y - epsilon;
    let max_surface_y = feet_pos.y + radius + penetration_tol;

    let footprint_min_x = feet_pos.x - radius;
    let footprint_max_x = feet_pos.x + radius;
    let footprint_min_z = feet_pos.z - radius;
    let footprint_max_z = feet_pos.z + radius;

    if let Some(runtime) = physics {
        struct DynCandidate {
            surface_y: f32,
            stable_feet_y: f32,
            vertical_dist: f32,
            abs_vertical_dist: f32,
            horiz_dist_sq: f32,
            body_id: crate::physics::DynamicBodyId,
        }

        let mut best_dyn: Option<DynCandidate> = None;

        for body in runtime.bodies.values() {
            let (b_min, b_max) = body.world_bounds();
            // Early-rejection AABB seluruh body
            if footprint_max_x < b_min.x
                || footprint_min_x > b_max.x
                || footprint_max_z < b_min.z
                || footprint_min_z > b_max.z
                || max_surface_y < b_min.y
                || min_surface_y > b_max.y + VOXEL_SIZE
            {
                continue;
            }

            // Batasi pencarian voxel lokal aggregate secara spasial ke irisan footprint (BLOCKING AMENDMENT 3)
            let rel_min_x =
                (((footprint_min_x - body.position.x) / VOXEL_SIZE).floor() as i32).max(0);
            let rel_max_x = ((footprint_max_x - body.position.x) / VOXEL_SIZE).floor() as i32;
            let rel_min_z =
                (((footprint_min_z - body.position.z) / VOXEL_SIZE).floor() as i32).max(0);
            let rel_max_z = ((footprint_max_z - body.position.z) / VOXEL_SIZE).floor() as i32;
            let rel_min_y = (((min_surface_y - VOXEL_SIZE - body.position.y) / VOXEL_SIZE).floor()
                as i32)
                .max(0);
            let rel_max_y = ((max_surface_y - body.position.y) / VOXEL_SIZE).floor() as i32;

            for v in &body.aggregate.voxels {
                if v.relative_coord.x < rel_min_x
                    || v.relative_coord.x > rel_max_x
                    || v.relative_coord.z < rel_min_z
                    || v.relative_coord.z > rel_max_z
                    || v.relative_coord.y < rel_min_y
                    || v.relative_coord.y > rel_max_y
                {
                    continue;
                }

                // Periksa apakah voxel ini memiliki permukaan atas yang terekspos (tidak tertutup voxel lain di atasnya)
                let above_rel = v.relative_coord + IVec3::Y;
                let has_voxel_above = body
                    .aggregate
                    .voxels
                    .iter()
                    .any(|other| other.relative_coord == above_rel);
                if has_voxel_above {
                    continue;
                }

                let box_min = body.position
                    + Vec3::new(
                        v.relative_coord.x as f32 * VOXEL_SIZE,
                        v.relative_coord.y as f32 * VOXEL_SIZE,
                        v.relative_coord.z as f32 * VOXEL_SIZE,
                    );
                let box_max = box_min + Vec3::splat(VOXEL_SIZE);
                let surface_y = box_max.y;

                if surface_y < min_surface_y || surface_y > max_surface_y {
                    continue;
                }

                // Pastikan di dunia statis di atas voxel ini adalah Known Air
                let world_above_center = Vec3::new(
                    (box_min.x + box_max.x) * 0.5,
                    surface_y + 0.1,
                    (box_min.z + box_max.z) * 0.5,
                );
                let world_above_voxel = crate::coord::world_pos_to_world_voxel(world_above_center);
                let above_query = store.get_voxel_world_checked(world_above_voxel);
                let is_air_above = match above_query {
                    Some(block) => block.is_air(),
                    None => false, // Unknown != Air
                };
                if !is_air_above {
                    continue;
                }

                let closest_x = feet_pos.x.clamp(box_min.x, box_max.x);
                let closest_z = feet_pos.z.clamp(box_min.z, box_max.z);
                let dx = feet_pos.x - closest_x;
                let dz = feet_pos.z - closest_z;
                let horiz_dist_sq = dx * dx + dz * dz;

                if horiz_dist_sq > (radius * radius) {
                    continue;
                }

                let y_offset = (radius * radius - horiz_dist_sq).max(0.0).sqrt();
                let capsule_bottom_at_contact = (feet_pos.y + radius) - y_offset;
                let vertical_dist = capsule_bottom_at_contact - surface_y;

                if vertical_dist < -penetration_tol || vertical_dist > epsilon {
                    continue;
                }

                let stable_feet_y = surface_y - (radius - y_offset);
                let abs_vertical_dist = vertical_dist.abs();

                let candidate = DynCandidate {
                    surface_y,
                    stable_feet_y,
                    vertical_dist,
                    abs_vertical_dist,
                    horiz_dist_sq,
                    body_id: body.id,
                };

                let is_better = match &best_dyn {
                    None => true,
                    Some(best) => {
                        let diff = candidate.abs_vertical_dist - best.abs_vertical_dist;
                        if diff < -1e-5 {
                            true
                        } else if diff > 1e-5 {
                            false
                        } else {
                            let horiz_diff = candidate.horiz_dist_sq - best.horiz_dist_sq;
                            if horiz_diff < -1e-5 {
                                true
                            } else if horiz_diff > 1e-5 {
                                false
                            } else {
                                candidate.body_id < best.body_id
                            }
                        }
                    }
                };

                if is_better {
                    best_dyn = Some(candidate);
                }
            }
        }

        if let Some(best) = best_dyn {
            return GroundContactResult {
                grounded: true,
                ground_normal: Vec3::Y,
                ground_distance: best.vertical_dist.max(0.0),
                support_voxel: None,
                ground_y_surface: Some(best.surface_y),
                stable_feet_y: Some(best.stable_feet_y),
            };
        }
    }

    GroundContactResult::default()
}

/// Memeriksa apakah kapsul dengan ketinggian target (misalnya standing_height = 1.8m)
/// memiliki ruang bebas penuh (clearance) tanpa beririsan dengan voxel solid statis (8B.5).
///
/// INVARIANTS:
/// - Menggunakan geometri kapsul penuh (Narrow-Phase), bukan hanya cek 1 voxel di atas kepala.
/// - Menghindari alokasi heap (Zero Allocation).
/// - Jika seluruh kandidat voxel adalah AIR (atau di luar jangkauan kapsul), mengembalikan `true`.
/// - Jika beririsan dengan balok solid atau chunk yang belum dimuat (Unknown), mengembalikan `false`.
pub fn check_capsule_clearance(
    feet_pos: Vec3,
    target_height: f32,
    radius: f32,
    store: &ChunkStore,
) -> bool {
    check_capsule_clearance_with_physics(feet_pos, target_height, radius, store, None)
}

/// Memeriksa clearance kapsul terhadap voxel statis dan voxel DynamicBody (8C.2).
pub fn check_capsule_clearance_with_physics(
    feet_pos: Vec3,
    target_height: f32,
    radius: f32,
    store: &ChunkStore,
    physics: Option<&PhysicsRuntime>,
) -> bool {
    let standing_capsule = super::collider::Capsule::new(feet_pos, radius, target_height);
    let (aabb_min, aabb_max) = standing_capsule.aabb();

    let vx_min = (aabb_min.x / VOXEL_SIZE).floor() as i32;
    let vx_max = (aabb_max.x / VOXEL_SIZE).floor() as i32;
    let vy_min = ((aabb_min.y + 0.05) / VOXEL_SIZE).floor() as i32;
    let vy_max = ((aabb_max.y - 0.01) / VOXEL_SIZE).floor() as i32;
    let vz_min = (aabb_min.z / VOXEL_SIZE).floor() as i32;
    let vz_max = (aabb_max.z / VOXEL_SIZE).floor() as i32;

    for vy in vy_min..=vy_max {
        for vz in vz_min..=vz_max {
            for vx in vx_min..=vx_max {
                let block_min = Vec3::new(
                    vx as f32 * VOXEL_SIZE,
                    vy as f32 * VOXEL_SIZE,
                    vz as f32 * VOXEL_SIZE,
                );
                let block_max = block_min + Vec3::splat(VOXEL_SIZE);

                let coord = IVec3::new(vx, vy, vz);
                match store.get_voxel_world_checked(coord) {
                    Some(block) => {
                        if !block.is_air() && standing_capsule.intersects_aabb(block_min, block_max)
                        {
                            return false;
                        }
                    }
                    None => {
                        if standing_capsule.intersects_aabb(block_min, block_max) {
                            return false;
                        }
                    }
                }
            }
        }
    }

    if let Some(runtime) = physics {
        for body in runtime.bodies.values() {
            let (b_min, b_max) = body.world_bounds();
            if aabb_max.x < b_min.x
                || aabb_min.x > b_max.x
                || aabb_max.y < b_min.y
                || aabb_min.y > b_max.y
                || aabb_max.z < b_min.z
                || aabb_min.z > b_max.z
            {
                continue;
            }

            for v in &body.aggregate.voxels {
                let box_min = body.position
                    + Vec3::new(
                        v.relative_coord.x as f32 * VOXEL_SIZE,
                        v.relative_coord.y as f32 * VOXEL_SIZE,
                        v.relative_coord.z as f32 * VOXEL_SIZE,
                    );
                let box_max = box_min + Vec3::splat(VOXEL_SIZE);
                if box_max.y <= feet_pos.y + 0.05 {
                    continue;
                }

                if standing_capsule.intersects_aabb(box_min, box_max) {
                    return false;
                }
            }
        }
    }

    true
}

/// Hasil evaluasi tabrakan kontinu 1D per sumbu (Swept Collision)
#[derive(Debug, Clone, Copy)]
pub struct SweptHit {
    pub hit: bool,
    pub t: f32,
    pub normal: Vec3,
    pub hit_voxel: Option<IVec3>,
    pub is_unknown: bool,
    pub surface_y: Option<f32>,
}

impl Default for SweptHit {
    fn default() -> Self {
        Self {
            hit: false,
            t: 1.0,
            normal: Vec3::ZERO,
            hit_voxel: None,
            is_unknown: false,
            surface_y: None,
        }
    }
}

/// Statistik resolusi tabrakan pada satu langkah pergerakan
#[derive(Debug, Clone, Copy, Default)]
pub struct CollisionStepStats {
    pub queries_count: u64,
    pub hits_count: u64,
    pub unknown_hits_count: u64,
}

/// Evaluasi swept collision kontinu sepanjang sumbu horizontal X (8B.8).
pub fn swept_axis_x(capsule: &super::collider::Capsule, dx: f32, store: &ChunkStore) -> SweptHit {
    if dx.abs() < 1e-6 {
        return SweptHit::default();
    }

    let radius = capsule.radius;
    let height = capsule.height;
    let base = capsule.base;

    let swept_min_x = (base.x - radius).min(base.x + dx - radius);
    let swept_max_x = (base.x + radius).max(base.x + dx + radius);

    let vx_min = (swept_min_x / VOXEL_SIZE).floor() as i32;
    let vx_max = (swept_max_x / VOXEL_SIZE).floor() as i32;

    let vy_min = ((base.y + 0.05) / VOXEL_SIZE).floor() as i32;
    let vy_max = ((base.y + height - 0.01) / VOXEL_SIZE).floor() as i32;

    let vz_min = ((base.z - radius) / VOXEL_SIZE).floor() as i32;
    let vz_max = ((base.z + radius) / VOXEL_SIZE).floor() as i32;

    let mut earliest_t = 1.0f32;
    let mut hit_normal = Vec3::ZERO;
    let mut hit_voxel = None;
    let mut is_unknown = false;

    for vy in vy_min..=vy_max {
        for vz in vz_min..=vz_max {
            for vx in vx_min..=vx_max {
                let box_min = Vec3::new(
                    vx as f32 * VOXEL_SIZE,
                    vy as f32 * VOXEL_SIZE,
                    vz as f32 * VOXEL_SIZE,
                );
                let box_max = box_min + Vec3::splat(VOXEL_SIZE);

                let coord = IVec3::new(vx, vy, vz);
                let voxel_query = store.get_voxel_world_checked(coord);
                let is_solid = match voxel_query {
                    Some(block) => !block.is_air(),
                    None => true, // Unknown != Air (blokir gerakan ke chunk belum dimuat)
                };

                if !is_solid {
                    continue;
                }

                // Periksa penampang YZ
                let closest_z = base.z.clamp(box_min.z, box_max.z);
                let dz = base.z - closest_z;

                let y_lower = base.y + radius;
                let y_upper = base.y + height - radius;
                let dy_seg = if y_upper < box_min.y {
                    box_min.y - y_upper
                } else if y_lower > box_max.y {
                    y_lower - box_max.y
                } else {
                    0.0
                };

                let cross_dist_sq = dz * dz + dy_seg * dy_seg;
                if cross_dist_sq > (radius * radius) {
                    continue;
                }

                let r_eff = (radius * radius - cross_dist_sq).max(0.0).sqrt();

                if dx > 0.0 {
                    let cur_front = base.x + r_eff;
                    let target_wall = box_min.x;
                    if cur_front <= target_wall {
                        let t = (target_wall - cur_front) / dx;
                        if t >= 0.0 && t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::NEG_X;
                            hit_voxel = Some(coord);
                            is_unknown = voxel_query.is_none();
                        }
                    } else if base.x < box_max.x && (base.x - r_eff) < box_max.x {
                        let t = 0.0f32;
                        if t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::NEG_X;
                            hit_voxel = Some(coord);
                            is_unknown = voxel_query.is_none();
                        }
                    }
                } else {
                    let cur_front = base.x - r_eff;
                    let target_wall = box_max.x;
                    if cur_front >= target_wall {
                        let t = (target_wall - cur_front) / dx;
                        if t >= 0.0 && t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::X;
                            hit_voxel = Some(coord);
                            is_unknown = voxel_query.is_none();
                        }
                    } else if base.x > box_min.x && (base.x + r_eff) > box_min.x {
                        let t = 0.0f32;
                        if t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::X;
                            hit_voxel = Some(coord);
                            is_unknown = voxel_query.is_none();
                        }
                    }
                }
            }
        }
    }

    if earliest_t < 1.0 {
        SweptHit {
            hit: true,
            t: earliest_t,
            normal: hit_normal,
            hit_voxel,
            is_unknown,
            surface_y: None,
        }
    } else {
        SweptHit::default()
    }
}

/// Evaluasi swept collision kontinu sepanjang sumbu horizontal Z (8B.8).
pub fn swept_axis_z(capsule: &super::collider::Capsule, dz: f32, store: &ChunkStore) -> SweptHit {
    if dz.abs() < 1e-6 {
        return SweptHit::default();
    }

    let radius = capsule.radius;
    let height = capsule.height;
    let base = capsule.base;

    let swept_min_z = (base.z - radius).min(base.z + dz - radius);
    let swept_max_z = (base.z + radius).max(base.z + dz + radius);

    let vz_min = (swept_min_z / VOXEL_SIZE).floor() as i32;
    let vz_max = (swept_max_z / VOXEL_SIZE).floor() as i32;

    let vy_min = ((base.y + 0.05) / VOXEL_SIZE).floor() as i32;
    let vy_max = ((base.y + height - 0.01) / VOXEL_SIZE).floor() as i32;

    let vx_min = ((base.x - radius) / VOXEL_SIZE).floor() as i32;
    let vx_max = ((base.x + radius) / VOXEL_SIZE).floor() as i32;

    let mut earliest_t = 1.0f32;
    let mut hit_normal = Vec3::ZERO;
    let mut hit_voxel = None;
    let mut is_unknown = false;

    for vy in vy_min..=vy_max {
        for vx in vx_min..=vx_max {
            for vz in vz_min..=vz_max {
                let box_min = Vec3::new(
                    vx as f32 * VOXEL_SIZE,
                    vy as f32 * VOXEL_SIZE,
                    vz as f32 * VOXEL_SIZE,
                );
                let box_max = box_min + Vec3::splat(VOXEL_SIZE);

                let coord = IVec3::new(vx, vy, vz);
                let voxel_query = store.get_voxel_world_checked(coord);
                let is_solid = match voxel_query {
                    Some(block) => !block.is_air(),
                    None => true, // Unknown != Air
                };

                if !is_solid {
                    continue;
                }

                // Periksa penampang XY
                let closest_x = base.x.clamp(box_min.x, box_max.x);
                let dx = base.x - closest_x;

                let y_lower = base.y + radius;
                let y_upper = base.y + height - radius;
                let dy_seg = if y_upper < box_min.y {
                    box_min.y - y_upper
                } else if y_lower > box_max.y {
                    y_lower - box_max.y
                } else {
                    0.0
                };

                let cross_dist_sq = dx * dx + dy_seg * dy_seg;
                if cross_dist_sq > (radius * radius) {
                    continue;
                }

                let r_eff = (radius * radius - cross_dist_sq).max(0.0).sqrt();

                if dz > 0.0 {
                    let cur_front = base.z + r_eff;
                    let target_wall = box_min.z;
                    if cur_front <= target_wall {
                        let t = (target_wall - cur_front) / dz;
                        if t >= 0.0 && t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::NEG_Z;
                            hit_voxel = Some(coord);
                            is_unknown = voxel_query.is_none();
                        }
                    } else if base.z < box_max.z && (base.z - r_eff) < box_max.z {
                        let t = 0.0f32;
                        if t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::NEG_Z;
                            hit_voxel = Some(coord);
                            is_unknown = voxel_query.is_none();
                        }
                    }
                } else {
                    let cur_front = base.z - r_eff;
                    let target_wall = box_max.z;
                    if cur_front >= target_wall {
                        let t = (target_wall - cur_front) / dz;
                        if t >= 0.0 && t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::Z;
                            hit_voxel = Some(coord);
                            is_unknown = voxel_query.is_none();
                        }
                    } else if base.z > box_min.z && (base.z + r_eff) > box_min.z {
                        let t = 0.0f32;
                        if t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::Z;
                            hit_voxel = Some(coord);
                            is_unknown = voxel_query.is_none();
                        }
                    }
                }
            }
        }
    }

    if earliest_t < 1.0 {
        SweptHit {
            hit: true,
            t: earliest_t,
            normal: hit_normal,
            hit_voxel,
            is_unknown,
            surface_y: None,
        }
    } else {
        SweptHit::default()
    }
}

/// Evaluasi swept collision kontinu sepanjang sumbu vertikal Y (8B.8).
pub fn swept_axis_y(capsule: &super::collider::Capsule, dy: f32, store: &ChunkStore) -> SweptHit {
    if dy.abs() < 1e-6 {
        return SweptHit::default();
    }

    let radius = capsule.radius;
    let height = capsule.height;
    let base = capsule.base;

    let swept_min_y = (base.y).min(base.y + dy);
    let swept_max_y = (base.y + height).max(base.y + dy + height);

    let vy_min = (swept_min_y / VOXEL_SIZE).floor() as i32;
    let vy_max = (swept_max_y / VOXEL_SIZE).floor() as i32;

    let vx_min = ((base.x - radius) / VOXEL_SIZE).floor() as i32;
    let vx_max = ((base.x + radius) / VOXEL_SIZE).floor() as i32;

    let vz_min = ((base.z - radius) / VOXEL_SIZE).floor() as i32;
    let vz_max = ((base.z + radius) / VOXEL_SIZE).floor() as i32;

    let mut earliest_t = 1.0f32;
    let mut hit_normal = Vec3::ZERO;
    let mut hit_voxel = None;
    let mut is_unknown = false;
    let mut hit_surface_y = None;

    for vy in vy_min..=vy_max {
        for vz in vz_min..=vz_max {
            for vx in vx_min..=vx_max {
                let box_min = Vec3::new(
                    vx as f32 * VOXEL_SIZE,
                    vy as f32 * VOXEL_SIZE,
                    vz as f32 * VOXEL_SIZE,
                );
                let box_max = box_min + Vec3::splat(VOXEL_SIZE);

                let coord = IVec3::new(vx, vy, vz);
                let voxel_query = store.get_voxel_world_checked(coord);
                let is_solid = match voxel_query {
                    Some(block) => !block.is_air(),
                    None => true, // Unknown != Air
                };

                if !is_solid {
                    continue;
                }

                // Penampang horizontal XZ
                let closest_x = base.x.clamp(box_min.x, box_max.x);
                let closest_z = base.z.clamp(box_min.z, box_max.z);
                let dx = base.x - closest_x;
                let dz = base.z - closest_z;
                let horiz_dist_sq = dx * dx + dz * dz;

                if horiz_dist_sq > (radius * radius) {
                    continue;
                }

                let y_offset = (radius * radius - horiz_dist_sq).max(0.0).sqrt();

                if dy < 0.0 {
                    let cur_bottom = (base.y + radius) - y_offset;
                    let target_floor = box_max.y;
                    if cur_bottom >= target_floor {
                        let t = (target_floor - cur_bottom) / dy;
                        if t >= 0.0 && t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::Y;
                            hit_voxel = Some(coord);
                            is_unknown = voxel_query.is_none();
                            hit_surface_y = Some(target_floor);
                        }
                    } else if (base.y + height) > box_min.y {
                        let t = 0.0f32;
                        if t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::Y;
                            hit_voxel = Some(coord);
                            is_unknown = voxel_query.is_none();
                            hit_surface_y = Some(target_floor);
                        }
                    }
                } else {
                    // Hanya periksa jika voxel berada di atas (kandidat langit-langit)
                    if base.y < box_min.y {
                        let cur_top = (base.y + height - radius) + y_offset;
                        let target_ceiling = box_min.y;
                        if cur_top <= target_ceiling {
                            let t = (target_ceiling - cur_top) / dy;
                            if t >= 0.0 && t < earliest_t {
                                earliest_t = t;
                                hit_normal = Vec3::NEG_Y;
                                hit_voxel = Some(coord);
                                is_unknown = voxel_query.is_none();
                            }
                        } else if base.y + height > target_ceiling {
                            let t = 0.0f32;
                            if t < earliest_t {
                                earliest_t = t;
                                hit_normal = Vec3::NEG_Y;
                                hit_voxel = Some(coord);
                                is_unknown = voxel_query.is_none();
                            }
                        }
                    }
                }
            }
        }
    }

    if earliest_t < 1.0 {
        SweptHit {
            hit: true,
            t: earliest_t,
            normal: hit_normal,
            hit_voxel,
            is_unknown,
            surface_y: hit_surface_y,
        }
    } else {
        SweptHit::default()
    }
}

/// Menyelesaikan pergerakan swept collision per sumbu berurutan: X -> Z -> Y (8B.8).
pub fn resolve_swept_step(
    capsule: &mut super::collider::Capsule,
    velocity: &mut Vec3,
    delta: Vec3,
    store: &ChunkStore,
) -> CollisionStepStats {
    let mut stats = CollisionStepStats::default();

    // 1. Sumbu X
    if delta.x.abs() > 1e-6 {
        stats.queries_count += 1;
        let hit_x = swept_axis_x(capsule, delta.x, store);
        if hit_x.hit {
            stats.hits_count += 1;
            if hit_x.is_unknown {
                stats.unknown_hits_count += 1;
            }
            let mut move_x = delta.x * hit_x.t;
            if delta.x > 0.0 {
                move_x = (move_x - 0.001).max(0.0);
            } else {
                move_x = (move_x + 0.001).min(0.0);
            }
            capsule.base.x += move_x;
            velocity.x = 0.0;
        } else {
            capsule.base.x += delta.x;
        }
    }

    // 2. Sumbu Z
    if delta.z.abs() > 1e-6 {
        stats.queries_count += 1;
        let hit_z = swept_axis_z(capsule, delta.z, store);
        if hit_z.hit {
            stats.hits_count += 1;
            if hit_z.is_unknown {
                stats.unknown_hits_count += 1;
            }
            let mut move_z = delta.z * hit_z.t;
            if delta.z > 0.0 {
                move_z = (move_z - 0.001).max(0.0);
            } else {
                move_z = (move_z + 0.001).min(0.0);
            }
            capsule.base.z += move_z;
            velocity.z = 0.0;
        } else {
            capsule.base.z += delta.z;
        }
    }

    // 3. Sumbu Y
    if delta.y.abs() > 1e-6 {
        stats.queries_count += 1;
        let hit_y = swept_axis_y(capsule, delta.y, store);
        if hit_y.hit {
            stats.hits_count += 1;
            if hit_y.is_unknown {
                stats.unknown_hits_count += 1;
            }
            let mut move_y = delta.y * hit_y.t;
            if delta.y > 0.0 {
                move_y = (move_y - 0.001).max(0.0);
            } else {
                move_y = (move_y + 0.001).min(0.0);
            }
            capsule.base.y += move_y;
            velocity.y = 0.0;
        } else {
            capsule.base.y += delta.y;
        }
    }

    stats
}

/// Evaluasi swept collision kontinu sepanjang sumbu X dengan dukungan PhysicsRuntime DynamicBody (8C.2).
pub fn swept_axis_x_with_physics(
    capsule: &super::collider::Capsule,
    dx: f32,
    store: &ChunkStore,
    physics: Option<&PhysicsRuntime>,
) -> SweptHit {
    let mut hit = swept_axis_x(capsule, dx, store);

    if let Some(runtime) = physics {
        if dx.abs() < 1e-6 {
            return hit;
        }

        let radius = capsule.radius;
        let height = capsule.height;
        let base = capsule.base;

        let swept_min_x = (base.x - radius).min(base.x + dx - radius);
        let swept_max_x = (base.x + radius).max(base.x + dx + radius);
        let c_min_y = base.y + 0.05;
        let c_max_y = base.y + height - 0.01;
        let c_min_z = base.z - radius;
        let c_max_z = base.z + radius;

        let mut earliest_t = if hit.hit { hit.t } else { 1.0f32 };
        let mut hit_normal = hit.normal;

        for body in runtime.bodies.values() {
            let (b_min, b_max) = body.world_bounds();
            if swept_max_x < b_min.x
                || swept_min_x > b_max.x
                || c_max_y < b_min.y
                || c_min_y > b_max.y
                || c_max_z < b_min.z
                || c_min_z > b_max.z
            {
                continue;
            }

            for v in &body.aggregate.voxels {
                let box_min = body.position
                    + Vec3::new(
                        v.relative_coord.x as f32 * VOXEL_SIZE,
                        v.relative_coord.y as f32 * VOXEL_SIZE,
                        v.relative_coord.z as f32 * VOXEL_SIZE,
                    );
                let box_max = box_min + Vec3::splat(VOXEL_SIZE);

                if box_max.y <= base.y + 0.05 {
                    continue;
                }

                let closest_z = base.z.clamp(box_min.z, box_max.z);
                let dz = base.z - closest_z;

                let y_lower = base.y + radius;
                let y_upper = base.y + height - radius;
                let dy_seg = if y_upper < box_min.y {
                    box_min.y - y_upper
                } else if y_lower > box_max.y {
                    y_lower - box_max.y
                } else {
                    0.0
                };

                let cross_dist_sq = dz * dz + dy_seg * dy_seg;
                if cross_dist_sq > (radius * radius) {
                    continue;
                }

                let r_eff = (radius * radius - cross_dist_sq).max(0.0).sqrt();

                if dx > 0.0 {
                    let cur_front = base.x + r_eff;
                    let target_wall = box_min.x;
                    if cur_front <= target_wall {
                        let t = (target_wall - cur_front) / dx;
                        if t >= 0.0 && t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::NEG_X;
                            hit.hit = true;
                            hit.hit_voxel = None;
                            hit.is_unknown = false;
                        }
                    } else if base.x < box_max.x && (base.x - r_eff) < box_max.x {
                        let t = 0.0f32;
                        if t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::NEG_X;
                            hit.hit = true;
                            hit.hit_voxel = None;
                            hit.is_unknown = false;
                        }
                    }
                } else {
                    let cur_front = base.x - r_eff;
                    let target_wall = box_max.x;
                    if cur_front >= target_wall {
                        let t = (target_wall - cur_front) / dx;
                        if t >= 0.0 && t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::X;
                            hit.hit = true;
                            hit.hit_voxel = None;
                            hit.is_unknown = false;
                        }
                    } else if base.x > box_min.x && (base.x + r_eff) > box_min.x {
                        let t = 0.0f32;
                        if t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::X;
                            hit.hit = true;
                            hit.hit_voxel = None;
                            hit.is_unknown = false;
                        }
                    }
                }
            }
        }

        if hit.hit {
            hit.t = earliest_t;
            hit.normal = hit_normal;
        }
    }

    hit
}

/// Evaluasi swept collision kontinu sepanjang sumbu Z dengan dukungan PhysicsRuntime DynamicBody (8C.2).
pub fn swept_axis_z_with_physics(
    capsule: &super::collider::Capsule,
    dz: f32,
    store: &ChunkStore,
    physics: Option<&PhysicsRuntime>,
) -> SweptHit {
    let mut hit = swept_axis_z(capsule, dz, store);

    if let Some(runtime) = physics {
        if dz.abs() < 1e-6 {
            return hit;
        }

        let radius = capsule.radius;
        let height = capsule.height;
        let base = capsule.base;

        let swept_min_z = (base.z - radius).min(base.z + dz - radius);
        let swept_max_z = (base.z + radius).max(base.z + dz + radius);
        let c_min_y = base.y + 0.05;
        let c_max_y = base.y + height - 0.01;
        let c_min_x = base.x - radius;
        let c_max_x = base.x + radius;

        let mut earliest_t = if hit.hit { hit.t } else { 1.0f32 };
        let mut hit_normal = hit.normal;

        for body in runtime.bodies.values() {
            let (b_min, b_max) = body.world_bounds();
            if swept_max_z < b_min.z
                || swept_min_z > b_max.z
                || c_max_y < b_min.y
                || c_min_y > b_max.y
                || c_max_x < b_min.x
                || c_min_x > b_max.x
            {
                continue;
            }

            for v in &body.aggregate.voxels {
                let box_min = body.position
                    + Vec3::new(
                        v.relative_coord.x as f32 * VOXEL_SIZE,
                        v.relative_coord.y as f32 * VOXEL_SIZE,
                        v.relative_coord.z as f32 * VOXEL_SIZE,
                    );
                let box_max = box_min + Vec3::splat(VOXEL_SIZE);

                if box_max.y <= base.y + 0.05 {
                    continue;
                }

                let closest_x = base.x.clamp(box_min.x, box_max.x);
                let dx = base.x - closest_x;

                let y_lower = base.y + radius;
                let y_upper = base.y + height - radius;
                let dy_seg = if y_upper < box_min.y {
                    box_min.y - y_upper
                } else if y_lower > box_max.y {
                    y_lower - box_max.y
                } else {
                    0.0
                };

                let cross_dist_sq = dx * dx + dy_seg * dy_seg;
                if cross_dist_sq > (radius * radius) {
                    continue;
                }

                let r_eff = (radius * radius - cross_dist_sq).max(0.0).sqrt();

                if dz > 0.0 {
                    let cur_front = base.z + r_eff;
                    let target_wall = box_min.z;
                    if cur_front <= target_wall {
                        let t = (target_wall - cur_front) / dz;
                        if t >= 0.0 && t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::NEG_Z;
                            hit.hit = true;
                            hit.hit_voxel = None;
                            hit.is_unknown = false;
                        }
                    } else if base.z < box_max.z && (base.z - r_eff) < box_max.z {
                        let t = 0.0f32;
                        if t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::NEG_Z;
                            hit.hit = true;
                            hit.hit_voxel = None;
                            hit.is_unknown = false;
                        }
                    }
                } else {
                    let cur_front = base.z - r_eff;
                    let target_wall = box_max.z;
                    if cur_front >= target_wall {
                        let t = (target_wall - cur_front) / dz;
                        if t >= 0.0 && t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::Z;
                            hit.hit = true;
                            hit.hit_voxel = None;
                            hit.is_unknown = false;
                        }
                    } else if base.z > box_min.z && (base.z + r_eff) > box_min.z {
                        let t = 0.0f32;
                        if t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::Z;
                            hit.hit = true;
                            hit.hit_voxel = None;
                            hit.is_unknown = false;
                        }
                    }
                }
            }
        }

        if hit.hit {
            hit.t = earliest_t;
            hit.normal = hit_normal;
        }
    }

    hit
}

/// Evaluasi swept collision kontinu sepanjang sumbu Y dengan dukungan PhysicsRuntime DynamicBody (8C.2).
pub fn swept_axis_y_with_physics(
    capsule: &super::collider::Capsule,
    dy: f32,
    store: &ChunkStore,
    physics: Option<&PhysicsRuntime>,
) -> SweptHit {
    let mut hit = swept_axis_y(capsule, dy, store);

    if let Some(runtime) = physics {
        if dy.abs() < 1e-6 {
            return hit;
        }

        let radius = capsule.radius;
        let height = capsule.height;
        let base = capsule.base;

        let swept_min_y = base.y.min(base.y + dy);
        let swept_max_y = (base.y + height).max(base.y + dy + height);
        let c_min_x = base.x - radius;
        let c_max_x = base.x + radius;
        let c_min_z = base.z - radius;
        let c_max_z = base.z + radius;

        let mut earliest_t = if hit.hit { hit.t } else { 1.0f32 };
        let mut hit_normal = hit.normal;
        let mut hit_surface_y = hit.surface_y;

        for body in runtime.bodies.values() {
            let (b_min, b_max) = body.world_bounds();
            if swept_max_y < b_min.y
                || swept_min_y > b_max.y
                || c_max_x < b_min.x
                || c_min_x > b_max.x
                || c_max_z < b_min.z
                || c_min_z > b_max.z
            {
                continue;
            }

            for v in &body.aggregate.voxels {
                let box_min = body.position
                    + Vec3::new(
                        v.relative_coord.x as f32 * VOXEL_SIZE,
                        v.relative_coord.y as f32 * VOXEL_SIZE,
                        v.relative_coord.z as f32 * VOXEL_SIZE,
                    );
                let box_max = box_min + Vec3::splat(VOXEL_SIZE);

                // Penampang horizontal XZ
                let closest_x = base.x.clamp(box_min.x, box_max.x);
                let closest_z = base.z.clamp(box_min.z, box_max.z);
                let dx = base.x - closest_x;
                let dz = base.z - closest_z;
                let horiz_dist_sq = dx * dx + dz * dz;

                if horiz_dist_sq > (radius * radius) {
                    continue;
                }

                let y_offset = (radius * radius - horiz_dist_sq).max(0.0).sqrt();

                if dy < 0.0 {
                    // Hanya periksa jika voxel berada di bawah telapak kaki (kandidat lantai)
                    if base.y + height > box_max.y {
                        let cur_bottom = (base.y + radius) - y_offset;
                        let target_floor = box_max.y;
                        if cur_bottom >= target_floor {
                            let t = (target_floor - cur_bottom) / dy;
                            if t >= 0.0 && t < earliest_t {
                                earliest_t = t;
                                hit_normal = Vec3::Y;
                                hit.hit = true;
                                hit.hit_voxel = None;
                                hit.is_unknown = false;
                                hit_surface_y = Some(target_floor);
                            }
                        } else if base.y < target_floor {
                            let t = 0.0f32;
                            if t < earliest_t {
                                earliest_t = t;
                                hit_normal = Vec3::Y;
                                hit.hit = true;
                                hit.hit_voxel = None;
                                hit.is_unknown = false;
                                hit_surface_y = Some(target_floor);
                            }
                        }
                    }
                } else {
                    // Hanya periksa jika voxel berada di atas kepala (kandidat langit-langit)
                    if base.y < box_min.y {
                        let cur_top = (base.y + height - radius) + y_offset;
                        let target_ceiling = box_min.y;
                        if cur_top <= target_ceiling {
                            let t = (target_ceiling - cur_top) / dy;
                            if t >= 0.0 && t < earliest_t {
                                earliest_t = t;
                                hit_normal = Vec3::NEG_Y;
                                hit.hit = true;
                                hit.hit_voxel = None;
                                hit.is_unknown = false;
                            }
                        } else if base.y + height > target_ceiling {
                            let t = 0.0f32;
                            if t < earliest_t {
                                earliest_t = t;
                                hit_normal = Vec3::NEG_Y;
                                hit.hit = true;
                                hit.hit_voxel = None;
                                hit.is_unknown = false;
                            }
                        }
                    }
                }
            }
        }

        if hit.hit {
            hit.t = earliest_t;
            hit.normal = hit_normal;
            hit.surface_y = hit_surface_y;
        }
    }

    hit
}

/// Menyelesaikan pergerakan swept collision per sumbu berurutan: X -> Z -> Y dengan dukungan PhysicsRuntime (8C.2).
pub fn resolve_swept_step_with_physics(
    capsule: &mut super::collider::Capsule,
    velocity: &mut Vec3,
    delta: Vec3,
    store: &ChunkStore,
    physics: Option<&PhysicsRuntime>,
) -> CollisionStepStats {
    let mut stats = CollisionStepStats::default();

    // 1. Sumbu X
    if delta.x.abs() > 1e-6 {
        stats.queries_count += 1;
        let hit_x = swept_axis_x_with_physics(capsule, delta.x, store, physics);
        if hit_x.hit {
            stats.hits_count += 1;
            if hit_x.is_unknown {
                stats.unknown_hits_count += 1;
            }
            let mut move_x = delta.x * hit_x.t;
            if delta.x > 0.0 {
                move_x = (move_x - 0.001).max(0.0);
            } else {
                move_x = (move_x + 0.001).min(0.0);
            }
            capsule.base.x += move_x;
            velocity.x = 0.0;
        } else {
            capsule.base.x += delta.x;
        }
    }

    // 2. Sumbu Z
    if delta.z.abs() > 1e-6 {
        stats.queries_count += 1;
        let hit_z = swept_axis_z_with_physics(capsule, delta.z, store, physics);
        if hit_z.hit {
            stats.hits_count += 1;
            if hit_z.is_unknown {
                stats.unknown_hits_count += 1;
            }
            let mut move_z = delta.z * hit_z.t;
            if delta.z > 0.0 {
                move_z = (move_z - 0.001).max(0.0);
            } else {
                move_z = (move_z + 0.001).min(0.0);
            }
            capsule.base.z += move_z;
            velocity.z = 0.0;
        } else {
            capsule.base.z += delta.z;
        }
    }

    // 3. Sumbu Y
    if delta.y.abs() > 1e-6 {
        stats.queries_count += 1;
        let hit_y = swept_axis_y_with_physics(capsule, delta.y, store, physics);
        if hit_y.hit {
            stats.hits_count += 1;
            if hit_y.is_unknown {
                stats.unknown_hits_count += 1;
            }
            let mut move_y = delta.y * hit_y.t;
            if delta.y > 0.0 {
                move_y = (move_y - 0.001).max(0.0);
            } else {
                move_y = (move_y + 0.001).min(0.0);
            }
            capsule.base.y += move_y;
            velocity.y = 0.0;
        } else {
            capsule.base.y += delta.y;
        }
    }

    stats
}

/// Mencoba auto-step / step-up melewati rintangan horizontal rendah (<= step_height) berdasarkan geometri aktual (8D.1).
///
/// INVARIANTS:
/// - AMENDMENT 1: Berbasis geometri sejati (geometry-based), bukan travel distance heuristic.
/// - AMENDMENT 2: Membedakan stepable ledge (0.5m) dari dinding penuh (>= 1.0m) lewat rise support surface (0 < rise <= step_height).
/// - AMENDMENT 3: Mencegah pemanjatan dinding vertikal datar tak berujung (no infinite wall climbing).
/// - AMENDMENT 4: Sapuan ke bawah dibatasi ketat (bounded step-down) pada rentang [0..step_height] dari posisi awal kaki.
/// - AMENDMENT 5: Memanfaatkan sepenuhnya kueri tabrakan kapsul dan voxel yang sudah ada tanpa menduplikasi engine.
/// - AMENDMENT 6: Bersifat atomik (transaksional): jika gagal pada tahap mana pun, tidak ada mutasi parsial.
/// - AMENDMENT 7: Hanya dipanggil saat pemain grounded (tidak aktif saat airborne).
/// - AMENDMENT 9: Dynamic bodies dievaluasi berdasarkan voxel agregat terisi (bukan AABB solid).
/// - AMENDMENT 10: Unknown chunk boundary diperlakukan sebagai pembatas keras (Unknown != Air).
pub fn try_step_up_with_physics(
    initial_capsule: &super::collider::Capsule,
    initial_velocity: Vec3,
    delta_h: Vec3,
    step_height: f32,
    store: &ChunkStore,
    physics: Option<&PhysicsRuntime>,
) -> Option<(super::collider::Capsule, Vec3, CollisionStepStats)> {
    if step_height <= 0.0 || (delta_h.x.abs() < 1e-6 && delta_h.z.abs() < 1e-6) {
        return None;
    }

    let mut step_stats = CollisionStepStats::default();

    // 1. TAHAP 1: Sapu kapsul ke atas untuk memeriksa headroom (ruang bebas vertikal)
    step_stats.queries_count += 1;
    let hit_up = swept_axis_y_with_physics(initial_capsule, step_height, store, physics);
    if hit_up.is_unknown {
        // AMENDMENT 10: Unknown chunk boundary ditolak keras
        return None;
    }

    let available_lift = if hit_up.hit {
        (step_height * hit_up.t - 0.001).max(0.0)
    } else {
        step_height
    };

    // Jika headroom kurang dari toleransi minimum (10cm), step tidak memungkinkan
    if available_lift < 0.10 {
        return None;
    }

    let mut elevated_capsule = *initial_capsule;
    elevated_capsule.base.y += available_lift;

    // 2. TAHAP 2: Validasi clearance kapsul di posisi terangkat
    step_stats.queries_count += 1;
    if !check_capsule_clearance_with_physics(
        elevated_capsule.base,
        elevated_capsule.height,
        elevated_capsule.radius,
        store,
        physics,
    ) {
        return None;
    }

    // 3. TAHAP 3: Sapuan horizontal pada ketinggian terangkat melintasi obstacle
    let mut horiz_capsule = elevated_capsule;
    let mut horiz_vel = initial_velocity;

    // Sumbu X pada posisi terangkat
    if delta_h.x.abs() > 1e-6 {
        step_stats.queries_count += 1;
        let hit_x = swept_axis_x_with_physics(&horiz_capsule, delta_h.x, store, physics);
        if hit_x.is_unknown {
            return None;
        }
        if hit_x.hit {
            step_stats.hits_count += 1;
            let mut mx = delta_h.x * hit_x.t;
            if delta_h.x > 0.0 {
                mx = (mx - 0.001).max(0.0);
            } else {
                mx = (mx + 0.001).min(0.0);
            }
            horiz_capsule.base.x += mx;
            horiz_vel.x = 0.0;
        } else {
            horiz_capsule.base.x += delta_h.x;
        }
    }

    // Sumbu Z pada posisi terangkat
    if delta_h.z.abs() > 1e-6 {
        step_stats.queries_count += 1;
        let hit_z = swept_axis_z_with_physics(&horiz_capsule, delta_h.z, store, physics);
        if hit_z.is_unknown {
            return None;
        }
        if hit_z.hit {
            step_stats.hits_count += 1;
            let mut mz = delta_h.z * hit_z.t;
            if delta_h.z > 0.0 {
                mz = (mz - 0.001).max(0.0);
            } else {
                mz = (mz + 0.001).min(0.0);
            }
            horiz_capsule.base.z += mz;
            horiz_vel.z = 0.0;
        } else {
            horiz_capsule.base.z += delta_h.z;
        }
    }

    // Validasi apakah terjadi perpindahan horizontal nyata melewati rintangan
    let moved_x = (horiz_capsule.base.x - initial_capsule.base.x).abs();
    let moved_z = (horiz_capsule.base.z - initial_capsule.base.z).abs();
    if moved_x < 0.001 && moved_z < 0.001 {
        // Dinding terlalu tinggi bahkan setelah diangkat (misal dinding 1.0m+ atau dinding datar tak berujung)
        return None;
    }

    // 4. TAHAP 4: Sapu ke bawah untuk mendeteksi permukaan tumpuan solid (AMENDMENT 2 & 4)
    // Sapuan dibatasi ketat agar tidak mencari tumpuan di bawah ketinggian telapak kaki awal
    let down_sweep = -(available_lift + 0.05);
    step_stats.queries_count += 1;
    let hit_down = swept_axis_y_with_physics(&horiz_capsule, down_sweep, store, physics);
    if hit_down.is_unknown || !hit_down.hit {
        // Tidak ada permukaan tumpuan solid di bawah (misal tepi jurang / void)
        return None;
    }

    let candidate_y = if let Some(surf) = hit_down.surface_y {
        surf
    } else {
        let move_down = down_sweep * hit_down.t + 0.001;
        horiz_capsule.base.y + move_down
    };

    // AMENDMENT 2 & 4: Hitung vertical rise terhadap telapak kaki awal
    let vertical_rise = candidate_y - initial_capsule.base.y;
    // Rise harus positif (permukaan lebih tinggi dari awal) dan <= step_height + toleransi
    if vertical_rise < 0.01 || vertical_rise > step_height + 0.02 {
        return None;
    }

    let mut candidate_capsule = horiz_capsule;
    candidate_capsule.base.y = candidate_y;

    // 5. TAHAP 5: Verifikasi ground support aktual dan clearance kapsul penuh pada posisi akhir
    step_stats.queries_count += 1;
    let ground_check = check_ground_support_with_physics(
        candidate_capsule.base,
        candidate_capsule.radius,
        0.05,
        store,
        physics,
    );
    if !ground_check.grounded {
        return None;
    }

    step_stats.queries_count += 1;
    if !check_capsule_clearance_with_physics(
        candidate_capsule.base,
        candidate_capsule.height,
        candidate_capsule.radius,
        store,
        physics,
    ) {
        return None;
    }

    // 6. TAHAP 6: COMMIT ATOMIK (AMENDMENT 6)
    // Seluruh kondisi geometris terpenuhi secara sempurna
    Some((candidate_capsule, horiz_vel, step_stats))
}

/// Menyelesaikan pergerakan swept collision dengan dukungan auto-step geometry-based (8D.1).
pub fn resolve_swept_step_with_stepup(
    capsule: &mut super::collider::Capsule,
    velocity: &mut Vec3,
    delta: Vec3,
    step_height: f32,
    is_grounded: bool,
    store: &ChunkStore,
    physics: Option<&PhysicsRuntime>,
) -> CollisionStepStats {
    let initial_capsule = *capsule;
    let initial_velocity = *velocity;

    // 1. Jalankan resolusi normal sumbu X -> Z -> Y
    let mut normal_capsule = *capsule;
    let mut normal_velocity = *velocity;
    let stats = resolve_swept_step_with_physics(
        &mut normal_capsule,
        &mut normal_velocity,
        delta,
        store,
        physics,
    );

    // 2. Evaluasi apakah perlu auto-step (AMENDMENT 7):
    // Hanya jika pemain grounded, ada perpindahan horizontal yang diinginkan, terjadi hambatan horizontal, dan step_height > 0
    let delta_h = Vec3::new(delta.x, 0.0, delta.z);
    let hit_horizontal = (normal_capsule.base.x - (initial_capsule.base.x + delta.x)).abs() > 1e-4
        || (normal_capsule.base.z - (initial_capsule.base.z + delta.z)).abs() > 1e-4;

    if is_grounded && hit_horizontal && step_height > 0.0 && delta_h.length_squared() > 1e-6 {
        if let Some((stepped_capsule, stepped_velocity, step_stats)) = try_step_up_with_physics(
            &initial_capsule,
            initial_velocity,
            delta_h,
            step_height,
            store,
            physics,
        ) {
            *capsule = stepped_capsule;
            *velocity = stepped_velocity;
            // Sertakan pergerakan Y awal jika ada dan bebas
            if delta.y.abs() > 1e-6 {
                let move_y_capsule = *capsule;
                let y_stats = swept_axis_y_with_physics(&move_y_capsule, delta.y, store, physics);
                if !y_stats.hit {
                    capsule.base.y += delta.y;
                }
            }
            return step_stats;
        }
    }

    // Jika tidak auto-step atau auto-step ditolak secara geometris, commit hasil normal (AMENDMENT 6: Atomic Rollback)
    *capsule = normal_capsule;
    *velocity = normal_velocity;
    stats
}
