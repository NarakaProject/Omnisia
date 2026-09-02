use crate::voxel::VOXEL_SIZE;
use glam::{IVec3, Vec3};

pub const CHUNK_SIZE: i32 = 32;
pub const CHUNK_SIZE_USIZE: usize = 32;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE_USIZE * CHUNK_SIZE_USIZE * CHUNK_SIZE_USIZE; // 32,768 voxel
pub const CHUNK_WORLD_SIZE: f32 = (CHUNK_SIZE as f32) * VOXEL_SIZE; // 16.0 meter

/// Mengonversi koordinat voxel global (world voxel) ke koordinat chunk dan koordinat lokal [0..31].
///
/// PENTING: Menggunakan matematika Euclidean division (`div_euclid` & `rem_euclid`)
/// untuk menjamin kebenaran pada koordinat negatif tanpa bug integer truncation.
/// Contoh:
/// - world_voxel -1  => chunk -1, local 31
/// - world_voxel -32 => chunk -1, local 0
/// - world_voxel -33 => chunk -2, local 31
#[inline(always)]
pub fn world_voxel_to_chunk_and_local(world_voxel: IVec3) -> (IVec3, IVec3) {
    let chunk_coord = IVec3::new(
        world_voxel.x.div_euclid(CHUNK_SIZE),
        world_voxel.y.div_euclid(CHUNK_SIZE),
        world_voxel.z.div_euclid(CHUNK_SIZE),
    );
    let local_coord = IVec3::new(
        world_voxel.x.rem_euclid(CHUNK_SIZE),
        world_voxel.y.rem_euclid(CHUNK_SIZE),
        world_voxel.z.rem_euclid(CHUNK_SIZE),
    );
    (chunk_coord, local_coord)
}

/// Mengonversi koordinat chunk dan koordinat lokal ke koordinat voxel global (world voxel)
#[inline(always)]
pub fn chunk_and_local_to_world_voxel(chunk_coord: IVec3, local_coord: IVec3) -> IVec3 {
    chunk_coord * CHUNK_SIZE + local_coord
}

/// Mengonversi posisi float dunia (meter) ke koordinat voxel global
#[inline(always)]
pub fn world_pos_to_world_voxel(world_pos: Vec3) -> IVec3 {
    IVec3::new(
        (world_pos.x / VOXEL_SIZE).floor() as i32,
        (world_pos.y / VOXEL_SIZE).floor() as i32,
        (world_pos.z / VOXEL_SIZE).floor() as i32,
    )
}

/// Mengonversi koordinat voxel dunia ke posisi origin dunia (meter) sudut minimum voxel
#[inline(always)]
pub fn world_voxel_to_world_pos(world_voxel: IVec3) -> Vec3 {
    Vec3::new(
        world_voxel.x as f32 * VOXEL_SIZE,
        world_voxel.y as f32 * VOXEL_SIZE,
        world_voxel.z as f32 * VOXEL_SIZE,
    )
}

/// Formula Kanonikal Tunggal untuk linear index dalam chunk 32x32x32:
/// `index = x + (y * 32) + (z * 1024)`
#[inline(always)]
pub fn canonical_linear_index(x: usize, y: usize, z: usize) -> usize {
    debug_assert!(
        x < CHUNK_SIZE_USIZE && y < CHUNK_SIZE_USIZE && z < CHUNK_SIZE_USIZE,
        "Indeks lokal melebihi batas chunk 32x32x32"
    );
    x + (y * CHUNK_SIZE_USIZE) + (z * CHUNK_SIZE_USIZE * CHUNK_SIZE_USIZE)
}

/// Inverse dari canonical linear index
#[inline(always)]
pub fn canonical_coords_from_index(index: usize) -> (usize, usize, usize) {
    debug_assert!(index < CHUNK_VOLUME, "Indeks linear melebihi CHUNK_VOLUME");
    let x = index % CHUNK_SIZE_USIZE;
    let y = (index / CHUNK_SIZE_USIZE) % CHUNK_SIZE_USIZE;
    let z = index / (CHUNK_SIZE_USIZE * CHUNK_SIZE_USIZE);
    (x, y, z)
}
