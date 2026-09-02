use glam::IVec3;

use crate::chunk::Chunk;
use crate::material::{MaterialId, MaterialRegistry};
use crate::modding::resource_id::ResourceId;
use crate::voxel::VoxelBlock;

/// Trait untuk generator dunia deterministik
pub trait ChunkGenerator: Send + Sync {
    fn generate_chunk(&self, coord: IVec3, registry: &MaterialRegistry) -> Chunk;
}

/// Implementasi demo generator terrain dengan bukit berkontur dan floating island anti-gravitasi
pub struct DemoChunkGenerator {
    pub seed: u64,
}

impl Default for DemoChunkGenerator {
    fn default() -> Self {
        Self { seed: 1337 }
    }
}

impl DemoChunkGenerator {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }
}

impl ChunkGenerator for DemoChunkGenerator {
    fn generate_chunk(&self, coord: IVec3, registry: &MaterialRegistry) -> Chunk {
        let mut chunk = Chunk::new(coord);

        let stone_id = registry
            .resolve_material_id(&ResourceId::core("stone").unwrap())
            .unwrap_or(MaterialId::STONE);
        let dirt_id = registry
            .resolve_material_id(&ResourceId::core("dirt").unwrap())
            .unwrap_or(MaterialId::DIRT);
        let grass_id = registry
            .resolve_material_id(&ResourceId::core("grass").unwrap())
            .unwrap_or(MaterialId::GRASS);
        let metal_frame_id = registry
            .resolve_material_id(&ResourceId::core("metal_frame").unwrap())
            .unwrap_or(MaterialId::METAL_FRAME);
        let gold_accent_id = registry
            .resolve_material_id(&ResourceId::core("gold_accent").unwrap())
            .unwrap_or(MaterialId::GOLD_ACCENT);
        let ag_core_id = registry
            .resolve_material_id(&ResourceId::core("ag_core_casing").unwrap())
            .unwrap_or(MaterialId::AG_CORE_CASING);

        // 1. Terrain Lapisan Bawah (y <= 0)
        if coord.y == 0 {
            for lz in 0..32 {
                for lx in 0..32 {
                    let wx = coord.x * 32 + lx as i32;
                    let wz = coord.z * 32 + lz as i32;

                    let height =
                        10 + (((wx as f32 * 0.15).sin() + (wz as f32 * 0.15).cos()) * 3.0) as usize;

                    for ly in 0..=height.min(31) {
                        let mat = if ly < height.saturating_sub(4) {
                            stone_id
                        } else if ly < height {
                            dirt_id
                        } else {
                            grass_id
                        };
                        chunk.set_voxel(lx, ly, lz, VoxelBlock::new(mat));
                    }
                }
            }
        } else if coord.y < 0 {
            // Dunia bawah tanah padat penuh dengan batu
            chunk.fill_material(stone_id);
        }

        // 2. Floating Anti-Gravity Island pada Chunk [0, 1, 0]
        if coord == IVec3::new(0, 1, 0) {
            // Dasar pulau (Stone & Dirt)
            for lz in 8..24 {
                for lx in 8..24 {
                    let dist_sq = ((lx as i32 - 16).pow(2) + (lz as i32 - 16).pow(2)) as f32;
                    if dist_sq < 48.0 {
                        chunk.set_voxel(lx, 6, lz, VoxelBlock::new(dirt_id));
                        chunk.set_voxel(lx, 7, lz, VoxelBlock::new(grass_id));
                    }
                }
            }

            // Struktur Penopang Metal Frame & Gold Accents
            for y in 8..14 {
                chunk.set_voxel(10, y, 10, VoxelBlock::new(metal_frame_id));
                chunk.set_voxel(22, y, 10, VoxelBlock::new(metal_frame_id));
                chunk.set_voxel(10, y, 22, VoxelBlock::new(metal_frame_id));
                chunk.set_voxel(22, y, 22, VoxelBlock::new(metal_frame_id));
            }

            // Balok penghubung atas
            for x in 10..=22 {
                chunk.set_voxel(x, 14, 10, VoxelBlock::new(gold_accent_id));
                chunk.set_voxel(x, 14, 22, VoxelBlock::new(gold_accent_id));
            }
            for z in 10..=22 {
                chunk.set_voxel(10, 14, z, VoxelBlock::new(gold_accent_id));
                chunk.set_voxel(22, 14, z, VoxelBlock::new(gold_accent_id));
            }

            // Anti-Gravity Core Casing di tengah
            for dy in 0..3 {
                for dz in 0..3 {
                    for dx in 0..3 {
                        chunk.set_voxel(15 + dx, 10 + dy, 15 + dz, VoxelBlock::new(ag_core_id));
                    }
                }
            }
        }

        // Reset dirty flags untuk chunk baru yang digenerate (siap dimeshing)
        chunk.dirty_flags = crate::chunk::dirty_flags::ALL;
        chunk
    }
}
