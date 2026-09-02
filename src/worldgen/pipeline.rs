use glam::IVec3;

use crate::chunk::Chunk;
use crate::material::MaterialRegistry;
use crate::streaming::generator::ChunkGenerator;

use super::caves::CaveSampler;
use super::config::WorldGenConfig;
use super::features::{FormationSampler, OreSampler, OverhangSampler};
use super::terrain::TerrainProfiler;
use super::voxelizer::{ChunkVoxelizer, ResolvedGenMaterials};

/// Generator dunia prosedural resmi Omnisia yang deterministic, seed-based, continuous, dan streamable
pub struct ProceduralWorldGenerator {
    pub config: WorldGenConfig,
    profiler: TerrainProfiler,
    caves: CaveSampler,
    overhangs: OverhangSampler,
    ores: OreSampler,
    formations: FormationSampler,
}

impl ProceduralWorldGenerator {
    pub fn new(config: WorldGenConfig) -> Self {
        let profiler = TerrainProfiler::new(config);
        let caves = CaveSampler::new(config);
        let overhangs = OverhangSampler::new(config);
        let ores = OreSampler::new(config.seed.raw());
        let formations = FormationSampler::new(config.seed.raw().wrapping_add(8888));

        Self {
            config,
            profiler,
            caves,
            overhangs,
            ores,
            formations,
        }
    }

    pub fn profiler(&self) -> &TerrainProfiler {
        &self.profiler
    }

    pub fn caves(&self) -> &CaveSampler {
        &self.caves
    }

    pub fn overhangs(&self) -> &OverhangSampler {
        &self.overhangs
    }

    pub fn ores(&self) -> &OreSampler {
        &self.ores
    }

    pub fn formations(&self) -> &FormationSampler {
        &self.formations
    }
}

impl ChunkGenerator for ProceduralWorldGenerator {
    fn generate_chunk(&self, coord: IVec3, registry: &MaterialRegistry) -> Chunk {
        let resolved_materials = ResolvedGenMaterials::resolve(registry)
            .expect("MaterialRegistry harus memuat material wajib generasi dunia (tidak boleh fallback diam-diam)");
        ChunkVoxelizer::voxelize(
            coord,
            &self.profiler,
            &self.caves,
            &self.overhangs,
            &self.ores,
            &self.formations,
            &resolved_materials,
        )
    }
}
