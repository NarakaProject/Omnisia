use std::collections::HashMap;
use glam::IVec3;

use crate::chunk::Chunk;
use crate::coord::world_voxel_to_chunk_and_local;
use crate::material::{MaterialId, MaterialRegistry};
use crate::voxel::VoxelBlock;

/// Representasi dunia runtime sparse
pub struct World {
    pub chunks: HashMap<IVec3, Chunk>,
    pub materials: MaterialRegistry,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            materials: MaterialRegistry::with_builtin_materials(),
        }
    }

    #[inline(always)]
    pub fn get_chunk(&self, coord: &IVec3) -> Option<&Chunk> {
        self.chunks.get(coord)
    }

    #[inline(always)]
    pub fn get_chunk_mut(&mut self, coord: &IVec3) -> Option<&mut Chunk> {
        self.chunks.get_mut(coord)
    }

    #[inline(always)]
    pub fn get_or_create_chunk(&mut self, coord: IVec3) -> &mut Chunk {
        self.chunks.entry(coord).or_insert_with(|| Chunk::new(coord))
    }

    /// Menetapkan voxel pada koordinat global dunia (world voxel)
    pub fn set_voxel_world(&mut self, world_voxel: IVec3, block: VoxelBlock) {
        let (chunk_coord, local_coord) = world_voxel_to_chunk_and_local(world_voxel);
        let chunk = self.get_or_create_chunk(chunk_coord);
        chunk.set_voxel(
            local_coord.x as usize,
            local_coord.y as usize,
            local_coord.z as usize,
            block,
        );
    }

    /// Mengambil voxel pada koordinat global dunia
    pub fn get_voxel_world(&self, world_voxel: IVec3) -> VoxelBlock {
        let (chunk_coord, local_coord) = world_voxel_to_chunk_and_local(world_voxel);
        if let Some(chunk) = self.chunks.get(&chunk_coord) {
            *chunk.get_voxel(
                local_coord.x as usize,
                local_coord.y as usize,
                local_coord.z as usize,
            )
        } else {
            VoxelBlock::AIR
        }
    }

    /// Menghasilkan Demo World: Terrain berundak dan Floating Island anti-gravitasi
    pub fn generate_demo_world(&mut self) {
        log::info!("Membangun Demo World Omnisia...");

        // 1. Terrain Statis Dasar (Chunk [0, 0, 0], [1, 0, 0], [0, 0, 1], [1, 0, 1])
        for cx in 0..2 {
            for cz in 0..2 {
                let chunk_pos = IVec3::new(cx, 0, cz);
                let chunk = self.get_or_create_chunk(chunk_pos);

                for lz in 0..32 {
                    for lx in 0..32 {
                        let wx = cx * 32 + lx as i32;
                        let wz = cz * 32 + lz as i32;

                        // Variasi ketinggian bukit kecil
                        let height = 10 + (((wx as f32 * 0.15).sin() + (wz as f32 * 0.15).cos()) * 3.0) as usize;

                        for ly in 0..=height.min(31) {
                            let mat = if ly < height.saturating_sub(4) {
                                MaterialId::STONE
                            } else if ly < height {
                                MaterialId::DIRT
                            } else {
                                MaterialId::GRASS
                            };

                            chunk.set_voxel(lx, ly, lz, VoxelBlock::new(mat));
                        }
                    }
                }
            }
        }

        // 2. Pulau Mengapung Dinamis / Anti-Gravity Structure (Chunk [0, 1, 0])
        // Terletak pada ketinggian Y = 38..48 (dalam micro-voxel)
        let island_chunk = self.get_or_create_chunk(IVec3::new(0, 1, 0));

        // Dasar pulau (Stone & Dirt)
        for lz in 8..24 {
            for lx in 8..24 {
                let dist_sq = ((lx as i32 - 16).pow(2) + (lz as i32 - 16).pow(2)) as f32;
                if dist_sq < 48.0 {
                    island_chunk.set_voxel(lx, 6, lz, VoxelBlock::new(MaterialId::DIRT));
                    island_chunk.set_voxel(lx, 7, lz, VoxelBlock::new(MaterialId::GRASS));
                }
            }
        }

        // Struktur Penopang Metal Frame & Gold Accents
        for y in 8..14 {
            island_chunk.set_voxel(10, y, 10, VoxelBlock::new(MaterialId::METAL_FRAME));
            island_chunk.set_voxel(22, y, 10, VoxelBlock::new(MaterialId::METAL_FRAME));
            island_chunk.set_voxel(10, y, 22, VoxelBlock::new(MaterialId::METAL_FRAME));
            island_chunk.set_voxel(22, y, 22, VoxelBlock::new(MaterialId::METAL_FRAME));
        }

        // Balok penghubung atas
        for x in 10..=22 {
            island_chunk.set_voxel(x, 14, 10, VoxelBlock::new(MaterialId::GOLD_ACCENT));
            island_chunk.set_voxel(x, 14, 22, VoxelBlock::new(MaterialId::GOLD_ACCENT));
        }
        for z in 10..=22 {
            island_chunk.set_voxel(10, 14, z, VoxelBlock::new(MaterialId::GOLD_ACCENT));
            island_chunk.set_voxel(22, 14, z, VoxelBlock::new(MaterialId::GOLD_ACCENT));
        }

        // Anti-Gravity Core Casing di tengah (Mengambang di tengah struktur)
        for dy in 0..3 {
            for dz in 0..3 {
                for dx in 0..3 {
                    island_chunk.set_voxel(
                        15 + dx,
                        10 + dy,
                        15 + dz,
                        VoxelBlock::new(MaterialId::AG_CORE_CASING),
                    );
                }
            }
        }

        log::info!(
            "Demo world berhasil dibangun dengan {} chunks",
            self.chunks.len()
        );
    }
}
