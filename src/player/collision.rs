use glam::{IVec3, Vec3};

use crate::streaming::store::ChunkStore;
use crate::voxel::VOXEL_SIZE;

/// Hasil evaluasi kontak tumpuan tanah (Ground Detection)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundContactResult {
    /// Apakah pemain bertumpu pada permukaan solid yang dimuat
    pub grounded: bool,
    /// Vektor normal permukaan tumpuan (default: (0, 1, 0))
    pub ground_normal: Vec3,
    /// Jarak vertikal dari telapak kaki ke permukaan tumpuan dalam meter
    pub ground_distance: f32,
    /// Koordinat voxel dunia yang memberikan tumpuan
    pub support_voxel: Option<IVec3>,
    /// Ketinggian Y permukaan atas tumpuan dalam meter
    pub ground_y_surface: Option<f32>,
}

impl Default for GroundContactResult {
    fn default() -> Self {
        Self {
            grounded: false,
            ground_normal: Vec3::Y,
            ground_distance: f32::INFINITY,
            support_voxel: None,
            ground_y_surface: None,
        }
    }
}

/// Memeriksa apakah telapak kaki pemain bertumpu pada permukaan tanah solid (8B.2).
///
/// ATURAN KANONIKAL:
/// - 1 voxel = 0.5 meter.
/// - Permukaan atas voxel Y berada di $y_{\text{surface}} = (Y + 1) \times 0.5\text{m}$.
/// - Grounded terjadi jika telapak kaki pemain ($y_{\text{feet}}$) berada di atas permukaan solid
///   dengan jarak vertikal $\le \text{epsilon}$.
/// - Bebas alokasi heap (Zero Allocation).
pub fn check_ground_support(
    feet_pos: Vec3,
    radius: f32,
    epsilon: f32,
    store: &ChunkStore,
) -> GroundContactResult {
    let vx_min = ((feet_pos.x - radius) / VOXEL_SIZE).floor() as i32;
    let vx_max = ((feet_pos.x + radius) / VOXEL_SIZE).floor() as i32;

    let vz_min = ((feet_pos.z - radius) / VOXEL_SIZE).floor() as i32;
    let vz_max = ((feet_pos.z + radius) / VOXEL_SIZE).floor() as i32;

    // Voxel kandidat tepat di bawah telapak kaki dalam jangkauan epsilon
    let vy = ((feet_pos.y - epsilon) / VOXEL_SIZE).floor() as i32;
    let surface_y = (vy + 1) as f32 * VOXEL_SIZE;

    // Pastikan posisi kaki berada tepat atau sedikit di atas permukaan (dalam rentang toleransi epsilon)
    let dist = feet_pos.y - surface_y;
    if dist < -0.01 || dist > epsilon {
        return GroundContactResult::default();
    }

    let mut found_support = false;
    let mut supporting_voxel = None;

    // Periksa footprint horizontal lingkaran dasar kaki terhadap kotak voxel [vx_min..=vx_max, vz_min..=vz_max]
    for vz in vz_min..=vz_max {
        for vx in vx_min..=vx_max {
            let box_min_x = vx as f32 * VOXEL_SIZE;
            let box_max_x = box_min_x + VOXEL_SIZE;
            let box_min_z = vz as f32 * VOXEL_SIZE;
            let box_max_z = box_min_z + VOXEL_SIZE;

            // Uji overlap lingkaran horizontal (radius) dengan AABB 2D horizontal kotak voxel
            let closest_x = feet_pos.x.clamp(box_min_x, box_max_x);
            let closest_z = feet_pos.z.clamp(box_min_z, box_max_z);
            let dx = feet_pos.x - closest_x;
            let dz = feet_pos.z - closest_z;

            if (dx * dx + dz * dz) <= (radius * radius) {
                let coord = IVec3::new(vx, vy, vz);
                if let Some(block) = store.get_voxel_world_checked(coord) {
                    if !block.is_air() {
                        found_support = true;
                        supporting_voxel = Some(coord);
                        break;
                    }
                }
            }
        }
        if found_support {
            break;
        }
    }

    if found_support {
        GroundContactResult {
            grounded: true,
            ground_normal: Vec3::Y,
            ground_distance: dist.max(0.0),
            support_voxel: supporting_voxel,
            ground_y_surface: Some(surface_y),
        }
    } else {
        GroundContactResult::default()
    }
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
    let standing_capsule = super::collider::Capsule::new(feet_pos, radius, target_height);
    let (aabb_min, aabb_max) = standing_capsule.aabb();

    let vx_min = (aabb_min.x / VOXEL_SIZE).floor() as i32;
    let vx_max = (aabb_max.x / VOXEL_SIZE).floor() as i32;
    let vy_min = (aabb_min.y / VOXEL_SIZE).floor() as i32;
    let vy_max = (aabb_max.y / VOXEL_SIZE).floor() as i32;
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
}

impl Default for SweptHit {
    fn default() -> Self {
        Self {
            hit: false,
            t: 1.0,
            normal: Vec3::ZERO,
            hit_voxel: None,
            is_unknown: false,
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

    let vy_min = ((base.y + 0.01) / VOXEL_SIZE).floor() as i32;
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

    let vy_min = ((base.y + 0.01) / VOXEL_SIZE).floor() as i32;
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
                        }
                    } else if (base.y + height) > box_min.y {
                        let t = 0.0f32;
                        if t < earliest_t {
                            earliest_t = t;
                            hit_normal = Vec3::Y;
                            hit_voxel = Some(coord);
                            is_unknown = voxel_query.is_none();
                        }
                    }
                } else {
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
                    } else if base.y < box_max.y {
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

    if earliest_t < 1.0 {
        SweptHit {
            hit: true,
            t: earliest_t,
            normal: hit_normal,
            hit_voxel,
            is_unknown,
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
