use glam::IVec3;
use std::collections::HashMap;

use crate::chunk::Chunk;
use crate::material::MaterialId;

/// Level of Detail untuk representasi dunia jauh (Far World)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LodLevel {
    /// LOD0: Full-Resolution Voxel Chunk (Authoritative Truth, 32³ voxels)
    Lod0 = 0,
    /// LOD1: Agregasi 2x2x2 voxel (16³ voxel per chunk)
    Lod1 = 1,
    /// LOD2: Agregasi 4x4x4 voxel (8³ voxel per chunk)
    Lod2 = 2,
    /// LOD3: Agregasi 8x8x8 voxel (4³ voxel per chunk)
    Lod3 = 3,
}

/// Sel voxel teragregasi untuk data LOD jauh
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AggregatedVoxel {
    pub dominant_material: MaterialId,
    pub occupancy_ratio: u8, // 0..255 (kepadatan voxel dalam rentang)
}

/// Kontrak arsitektur untuk representasi dunia jauh (Distant Representation).
///
/// INVARIANT:
/// 1. LOD adalah DERIVED representation, BUKAN authoritative world state.
/// 2. Data LOD tidak pernah disimpan di dalam struct `Chunk`.
/// 3. Jika cache LOD hilang, sistem dapat membangun ulang (*rebuild*) dari ChunkStore atau disk.
pub trait DistantRepresentation: Send + Sync {
    fn build_from_chunk(&mut self, chunk: &Chunk, level: LodLevel);
    fn get_aggregated_sample(&self, world_coord: IVec3, level: LodLevel)
        -> Option<AggregatedVoxel>;
    fn clear(&mut self);
}

/// Penyimpanan agregat LOD hierarkis (Architectural Ready untuk Phase lanjutan)
pub struct HierarchicalLodStore {
    lod_levels: HashMap<LodLevel, HashMap<IVec3, Vec<AggregatedVoxel>>>,
}

impl Default for HierarchicalLodStore {
    fn default() -> Self {
        Self::new()
    }
}

impl HierarchicalLodStore {
    pub fn new() -> Self {
        Self {
            lod_levels: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.lod_levels.values().map(|m| m.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.lod_levels.is_empty()
    }
}

impl DistantRepresentation for HierarchicalLodStore {
    fn build_from_chunk(&mut self, chunk: &Chunk, level: LodLevel) {
        if level == LodLevel::Lod0 {
            return;
        }

        // Sampling sederhana untuk kontrak agregasi
        let step = 1 << (level as usize);
        let size = 32 / step;
        let mut voxels = Vec::with_capacity(size * size * size);

        for z in (0..32).step_by(step) {
            for y in (0..32).step_by(step) {
                for x in (0..32).step_by(step) {
                    let block = chunk.get_voxel(x, y, z);
                    voxels.push(AggregatedVoxel {
                        dominant_material: block.material(),
                        occupancy_ratio: if block.is_air() { 0 } else { 255 },
                    });
                }
            }
        }

        let level_map = self.lod_levels.entry(level).or_default();
        level_map.insert(chunk.position, voxels);
    }

    fn get_aggregated_sample(
        &self,
        world_coord: IVec3,
        level: LodLevel,
    ) -> Option<AggregatedVoxel> {
        let chunk_pos = IVec3::new(
            world_coord.x.div_euclid(32),
            world_coord.y.div_euclid(32),
            world_coord.z.div_euclid(32),
        );

        let level_map = self.lod_levels.get(&level)?;
        let data = level_map.get(&chunk_pos)?;

        data.first().copied()
    }

    fn clear(&mut self) {
        self.lod_levels.clear();
    }
}
