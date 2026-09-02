use glam::IVec3;

use super::biome::BiomeType;
use super::caves::CaveSampler;
use super::config::WorldGenConfig;
use super::noise::hash3d;
use super::terrain::TerrainProfiler;
use super::voxelizer::ResolvedGenMaterials;
use crate::chunk::Chunk;
use crate::material::MaterialId;
use crate::voxel::VoxelBlock;

/// Spesies vegetasi deterministik
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VegetationSpecies {
    OakTree,
    PineTree,
    DesertShrub,
    TallGrass,
}

/// Sampler dan stamper vegetasi kanonikal berbasis world-space
pub struct VegetationSampler {
    seed: u64,
    sea_level: i32,
}

impl VegetationSampler {
    pub fn new(config: WorldGenConfig) -> Self {
        Self {
            seed: config.seed.raw().wrapping_add(9009),
            sea_level: config.sea_level,
        }
    }

    /// Melakukan stamping vegetasi deterministik pada chunk tertentu tanpa ketergantungan tetangga
    pub fn stamp_vegetation_to_chunk(
        &self,
        chunk_coord: IVec3,
        chunk: &mut Chunk,
        profiler: &TerrainProfiler,
        caves: &CaveSampler,
        mats: &ResolvedGenMaterials,
    ) {
        let chunk_min_x = chunk_coord.x * 32;
        let chunk_max_x = chunk_min_x + 31;
        let chunk_min_y = chunk_coord.y * 32;
        let chunk_max_y = chunk_min_y + 31;
        let chunk_min_z = chunk_coord.z * 32;
        let chunk_max_z = chunk_min_z + 31;

        // Radius maksimum fitur vegetasi dalam voxel (Oak/Pine canopy radius = 4)
        let max_radius = 4;

        let search_min_x = chunk_min_x - max_radius;
        let search_max_x = chunk_max_x + max_radius;
        let search_min_z = chunk_min_z - max_radius;
        let search_max_z = chunk_max_z + max_radius;

        // Grid sel pencarian kanonikal 8x8 voxel
        let cell_min_x = search_min_x.div_euclid(8);
        let cell_max_x = search_max_x.div_euclid(8);
        let cell_min_z = search_min_z.div_euclid(8);
        let cell_max_z = search_max_z.div_euclid(8);

        for cz in cell_min_z..=cell_max_z {
            for cx in cell_min_x..=cell_max_x {
                let cell_hash = hash3d(cx, 0, cz, self.seed);

                // Offset anchor di dalam sel 8x8 (posisi [0..=7])
                let anchor_x = cx * 8 + (cell_hash % 8) as i32;
                let anchor_z = cz * 8 + ((cell_hash >> 8) % 8) as i32;

                // Evaluasi profil medan makro pada anchor (koordinat voxel dunia)
                let pt = profiler.evaluate(anchor_x as f32, anchor_z as f32);
                let anchor_y = pt.surface_height_y.floor() as i32;

                // Validitas 1: Ketinggian tanah di atas permukaan laut (tidak berakar di air)
                if anchor_y <= self.sea_level {
                    continue;
                }

                // Validitas 2: Tidak berada di dalam rongga gua 3D
                if caves.is_cave(
                    anchor_x as f32,
                    anchor_y as f32,
                    anchor_z as f32,
                    pt.surface_height_y,
                ) {
                    continue;
                }

                // Pemilihan spesies dan probabilitas berdasarkan ekologi biome
                let species = match pt.biome {
                    BiomeType::Forest => {
                        if cell_hash % 100 < 65 {
                            Some(VegetationSpecies::OakTree)
                        } else if cell_hash % 100 < 85 {
                            Some(VegetationSpecies::TallGrass)
                        } else {
                            None
                        }
                    }
                    BiomeType::Plains => {
                        if cell_hash % 100 < 15 {
                            Some(VegetationSpecies::OakTree)
                        } else if cell_hash % 100 < 60 {
                            Some(VegetationSpecies::TallGrass)
                        } else {
                            None
                        }
                    }
                    BiomeType::Hills => {
                        if cell_hash % 100 < 30 {
                            Some(VegetationSpecies::PineTree)
                        } else if cell_hash % 100 < 50 {
                            Some(VegetationSpecies::TallGrass)
                        } else {
                            None
                        }
                    }
                    BiomeType::Mountains => {
                        if anchor_y < 50 && cell_hash % 100 < 35 {
                            Some(VegetationSpecies::PineTree)
                        } else if cell_hash % 100 < 45 {
                            Some(VegetationSpecies::DesertShrub)
                        } else {
                            None
                        }
                    }
                    BiomeType::SnowPeaks => {
                        if anchor_y < 42 && cell_hash % 100 < 20 {
                            Some(VegetationSpecies::PineTree)
                        } else {
                            None
                        }
                    }
                    BiomeType::Desert => {
                        if cell_hash % 100 < 25 {
                            Some(VegetationSpecies::DesertShrub)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                let Some(sp) = species else { continue };

                // Stamping fitur vegetasi ke dalam chunk jika voxel beririsan
                self.stamp_feature(
                    anchor_x,
                    anchor_y,
                    anchor_z,
                    sp,
                    cell_hash,
                    chunk,
                    chunk_min_x,
                    chunk_max_x,
                    chunk_min_y,
                    chunk_max_y,
                    chunk_min_z,
                    chunk_max_z,
                    mats,
                );
            }
        }
    }

    /// Stamping template vegetasi kanonikal ke dalam koordinat lokal chunk yang ditargetkan
    #[allow(clippy::too_many_arguments)]
    fn stamp_feature(
        &self,
        ax: i32,
        ay: i32,
        az: i32,
        species: VegetationSpecies,
        hash: u64,
        chunk: &mut Chunk,
        c_min_x: i32,
        c_max_x: i32,
        c_min_y: i32,
        c_max_y: i32,
        c_min_z: i32,
        c_max_z: i32,
        mats: &ResolvedGenMaterials,
    ) {
        match species {
            VegetationSpecies::OakTree => {
                let height = 4 + (hash % 3) as i32; // Tinggi 4..=6
                let canopy_radius = 2i32;
                let canopy_center_y = ay + height;

                // 1. Batang Kayu Oak (Trunk)
                for h in 1..=height {
                    let wy = ay + h;
                    if wy >= c_min_y
                        && wy <= c_max_y
                        && ax >= c_min_x
                        && ax <= c_max_x
                        && az >= c_min_z
                        && az <= c_max_z
                    {
                        let lx = (ax - c_min_x) as usize;
                        let ly = (wy - c_min_y) as usize;
                        let lz = (az - c_min_z) as usize;
                        let current_mat = chunk.get_voxel(lx, ly, lz).material();
                        if current_mat == MaterialId::AIR
                            || current_mat == mats.leaves_oak
                            || current_mat == mats.tall_grass
                        {
                            chunk.set_voxel(lx, ly, lz, VoxelBlock::new(mats.wood_oak));
                        }
                    }
                }

                // 2. Mahkota Daun Oak (Spherical Canopy)
                for dy in -canopy_radius..=canopy_radius {
                    let wy = canopy_center_y + dy;
                    if wy < c_min_y || wy > c_max_y {
                        continue;
                    }
                    for dz in -canopy_radius..=canopy_radius {
                        let wz = az + dz;
                        if wz < c_min_z || wz > c_max_z {
                            continue;
                        }
                        for dx in -canopy_radius..=canopy_radius {
                            let wx = ax + dx;
                            if wx < c_min_x || wx > c_max_x {
                                continue;
                            }

                            // Bentuk bola mahkota dedaunan
                            let dist_sq = dx * dx + dy * dy + dz * dz;
                            if dist_sq <= canopy_radius * canopy_radius + 1 {
                                let lx = (wx - c_min_x) as usize;
                                let ly = (wy - c_min_y) as usize;
                                let lz = (wz - c_min_z) as usize;

                                let current_mat = chunk.get_voxel(lx, ly, lz).material();
                                if current_mat == MaterialId::AIR || current_mat == mats.tall_grass
                                {
                                    chunk.set_voxel(lx, ly, lz, VoxelBlock::new(mats.leaves_oak));
                                }
                            }
                        }
                    }
                }
            }
            VegetationSpecies::PineTree => {
                let height = 6 + (hash % 4) as i32; // Tinggi 6..=9

                // 1. Batang Kayu Pine (Trunk)
                for h in 1..=height {
                    let wy = ay + h;
                    if wy >= c_min_y
                        && wy <= c_max_y
                        && ax >= c_min_x
                        && ax <= c_max_x
                        && az >= c_min_z
                        && az <= c_max_z
                    {
                        let lx = (ax - c_min_x) as usize;
                        let ly = (wy - c_min_y) as usize;
                        let lz = (az - c_min_z) as usize;
                        let current_mat = chunk.get_voxel(lx, ly, lz).material();
                        if current_mat == MaterialId::AIR
                            || current_mat == mats.leaves_pine
                            || current_mat == mats.tall_grass
                        {
                            chunk.set_voxel(lx, ly, lz, VoxelBlock::new(mats.wood_pine));
                        }
                    }
                }

                // 2. Mahkota Daun Pine (Conical Stepped Canopy)
                for h in 2..=(height + 1) {
                    let wy = ay + h;
                    if wy < c_min_y || wy > c_max_y {
                        continue;
                    }

                    // Radius dedaunan mengecil seiring bertambahnya ketinggian
                    let radius = if h == height + 1 {
                        0 // Pucuk kerucut
                    } else if h >= height - 1 {
                        1
                    } else if (height - h) % 2 == 0 {
                        2
                    } else {
                        1
                    };

                    for dz in -radius..=radius {
                        let wz = az + dz;
                        if wz < c_min_z || wz > c_max_z {
                            continue;
                        }
                        for dx in -radius..=radius {
                            let wx = ax + dx;
                            if wx < c_min_x || wx > c_max_x {
                                continue;
                            }

                            let dist_sq = dx * dx + dz * dz;
                            if dist_sq <= radius * radius + 1 {
                                let lx = (wx - c_min_x) as usize;
                                let ly = (wy - c_min_y) as usize;
                                let lz = (wz - c_min_z) as usize;

                                let current_mat = chunk.get_voxel(lx, ly, lz).material();
                                if current_mat == MaterialId::AIR || current_mat == mats.tall_grass
                                {
                                    chunk.set_voxel(lx, ly, lz, VoxelBlock::new(mats.leaves_pine));
                                }
                            }
                        }
                    }
                }
            }
            VegetationSpecies::DesertShrub => {
                let wy = ay + 1;
                if wy >= c_min_y
                    && wy <= c_max_y
                    && ax >= c_min_x
                    && ax <= c_max_x
                    && az >= c_min_z
                    && az <= c_max_z
                {
                    let lx = (ax - c_min_x) as usize;
                    let ly = (wy - c_min_y) as usize;
                    let lz = (az - c_min_z) as usize;
                    if chunk.get_voxel(lx, ly, lz).is_air() {
                        chunk.set_voxel(lx, ly, lz, VoxelBlock::new(mats.shrub));
                    }
                }
            }
            VegetationSpecies::TallGrass => {
                let wy = ay + 1;
                if wy >= c_min_y
                    && wy <= c_max_y
                    && ax >= c_min_x
                    && ax <= c_max_x
                    && az >= c_min_z
                    && az <= c_max_z
                {
                    let lx = (ax - c_min_x) as usize;
                    let ly = (wy - c_min_y) as usize;
                    let lz = (az - c_min_z) as usize;
                    if chunk.get_voxel(lx, ly, lz).is_air() {
                        chunk.set_voxel(lx, ly, lz, VoxelBlock::new(mats.tall_grass));
                    }
                }
            }
        }
    }
}
