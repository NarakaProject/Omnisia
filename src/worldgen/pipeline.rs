use glam::IVec3;

use crate::chunk::Chunk;
use crate::material::MaterialRegistry;
use crate::streaming::generator::ChunkGenerator;

use super::config::WorldGenConfig;
use super::terrain::TerrainProfiler;
use super::voxelizer::{ChunkVoxelizer, ResolvedGenMaterials};

/// Generator dunia prosedural resmi Omnisia yang deterministic, seed-based, continuous, dan streamable
pub struct ProceduralWorldGenerator {
    pub config: WorldGenConfig,
    profiler: TerrainProfiler,
}

impl ProceduralWorldGenerator {
    pub fn new(config: WorldGenConfig) -> Self {
        let profiler = TerrainProfiler::new(config);
        Self { config, profiler }
    }

    pub fn profiler(&self) -> &TerrainProfiler {
        &self.profiler
    }
}

impl ChunkGenerator for ProceduralWorldGenerator {
    fn generate_chunk(&self, coord: IVec3, registry: &MaterialRegistry) -> Chunk {
        let resolved_materials = ResolvedGenMaterials::resolve(registry);
        ChunkVoxelizer::voxelize(coord, &self.profiler, &resolved_materials)
    }
}
