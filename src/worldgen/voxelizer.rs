use glam::IVec3;

use crate::chunk::{dirty_flags, Chunk};
use crate::coord::CHUNK_SIZE_USIZE;
use crate::material::{MaterialId, MaterialRegistry};
use crate::modding::resource_id::ResourceId;
use crate::voxel::VoxelBlock;

use super::biome::BiomeType;
use super::terrain::TerrainProfiler;

/// Struktur cache ID material runtime untuk proses voxelization cepat tanpa alokasi / string parsing
#[derive(Debug, Clone, Copy)]
pub struct ResolvedGenMaterials {
    pub stone: MaterialId,
    pub dirt: MaterialId,
    pub grass: MaterialId,
    pub sand: MaterialId,
    pub water: MaterialId,
    pub snow: MaterialId,
}

impl ResolvedGenMaterials {
    pub fn resolve(registry: &MaterialRegistry) -> Self {
        let resolve_or = |name: &str, fallback: MaterialId| {
            ResourceId::core(name)
                .ok()
                .and_then(|res| registry.resolve_material_id(&res))
                .unwrap_or(fallback)
        };

        Self {
            stone: resolve_or("stone", MaterialId::STONE),
            dirt: resolve_or("dirt", MaterialId::DIRT),
            grass: resolve_or("grass", MaterialId::GRASS),
            sand: resolve_or("sand", MaterialId::STONE),
            water: resolve_or("water", MaterialId::AIR),
            snow: resolve_or("snow", MaterialId::STONE),
        }
    }
}

/// Voxelizer yang mengubah profil medan kontinu menjadi representasi 32³ micro-voxels
pub struct ChunkVoxelizer;

impl ChunkVoxelizer {
    pub fn voxelize(
        chunk_coord: IVec3,
        profiler: &TerrainProfiler,
        materials: &ResolvedGenMaterials,
    ) -> Chunk {
        let mut chunk = Chunk::new(chunk_coord);
        let mut non_air = 0u16;

        let base_world_x = chunk_coord.x * 32;
        let base_world_y = chunk_coord.y * 32;
        let base_world_z = chunk_coord.z * 32;

        // Evaluasi 2D kolom terrain (32x32 titik)
        let mut column_points = [[None; CHUNK_SIZE_USIZE]; CHUNK_SIZE_USIZE];
        for (lz, row) in column_points.iter_mut().enumerate() {
            let wz = (base_world_z + lz as i32) as f32;
            for (lx, cell) in row.iter_mut().enumerate() {
                let wx = (base_world_x + lx as i32) as f32;
                *cell = Some(profiler.evaluate(wx, wz));
            }
        }

        // Voxelization ke volume 32x32x32
        for (lz, row) in column_points.iter().enumerate() {
            for (lx, &cell) in row.iter().enumerate() {
                let pt = cell.unwrap();
                let surface_floor_y = pt.surface_height_y.floor() as i32;
                let water_floor_y = pt.water_level_y.floor() as i32;

                for ly in 0..CHUNK_SIZE_USIZE {
                    let world_y = base_world_y + ly as i32;

                    let mat = if world_y <= surface_floor_y {
                        // 1. Lapisan Padat (Solid Ground)
                        if world_y < surface_floor_y - 4 {
                            materials.stone
                        } else if world_y < surface_floor_y {
                            match pt.biome {
                                BiomeType::Desert | BiomeType::Beach => materials.sand,
                                BiomeType::SnowPeaks | BiomeType::Mountains => materials.stone,
                                _ => materials.dirt,
                            }
                        } else {
                            // Lapisan Permukaan Teratas (Surface Topsoil)
                            match pt.biome {
                                BiomeType::SnowPeaks => materials.snow,
                                BiomeType::Mountains => {
                                    if world_y > 45 {
                                        materials.snow
                                    } else {
                                        materials.stone
                                    }
                                }
                                BiomeType::Desert
                                | BiomeType::Beach
                                | BiomeType::Ocean
                                | BiomeType::DeepOcean => materials.sand,
                                _ => materials.grass,
                            }
                        }
                    } else if world_y <= water_floor_y {
                        // 2. Lapisan Air (Water Fluid)
                        materials.water
                    } else {
                        // 3. Udara Bebas (Air)
                        MaterialId::AIR
                    };

                    if mat != MaterialId::AIR {
                        chunk.set_voxel(lx, ly, lz, VoxelBlock::new(mat));
                        non_air += 1;
                    }
                }
            }
        }

        chunk.non_air_count = non_air;
        chunk.dirty_flags = dirty_flags::ALL;
        chunk
    }
}
