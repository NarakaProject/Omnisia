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
