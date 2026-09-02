use super::biome::{BiomeClassifier, BiomeType};
use super::climate::{ClimateSample, ClimateSampler};
use super::config::WorldGenConfig;
use super::hydrology::{HydrologySample, HydrologySampler};

/// Profil permukaan tanah pada koordinat dunia 2D
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainPoint {
    pub surface_height_y: f32, // Tinggi permukaan tanah dalam koordinat dunia Y (voxel units)
    pub water_level_y: f32,    // Tinggi permukaan air (voxel units)
    pub biome: BiomeType,
    pub climate: ClimateSample,
    pub hydrology: HydrologySample,
}

/// Profiler terrain global kontinu
pub struct TerrainProfiler {
    pub config: WorldGenConfig,
    climate_sampler: ClimateSampler,
    hydrology_sampler: HydrologySampler,
}

#[inline(always)]
fn sample_continental_spline(c: f32, sea_level_y: f32) -> f32 {
    // Titik kontrol spline benua: (continentalness, offset_terhadap_sea_level)
    const SPLINE_POINTS: [(f32, f32); 8] = [
        (-1.00, -22.0),
        (-0.45, -15.0),
        (-0.15, -6.0),
        (0.00, 0.0),  // Garis Pantai tepat di sea_level
        (0.12, 3.5),  // Dataran rendah
        (0.35, 8.0),  // Dataran bergelombang
        (0.65, 18.0), // Dasar perbukitan
        (1.00, 26.0), // Dasar pegunungan tinggi
    ];

    if c <= SPLINE_POINTS[0].0 {
        return sea_level_y + SPLINE_POINTS[0].1;
    }
    if c >= SPLINE_POINTS[SPLINE_POINTS.len() - 1].0 {
        return sea_level_y + SPLINE_POINTS[SPLINE_POINTS.len() - 1].1;
    }

    for i in 0..SPLINE_POINTS.len() - 1 {
        let (c0, h0) = SPLINE_POINTS[i];
        let (c1, h1) = SPLINE_POINTS[i + 1];
        if c >= c0 && c <= c1 {
            let t = (c - c0) / (c1 - c0);
            let smooth_t = t * t * (3.0 - 2.0 * t);
            let offset = h0 + smooth_t * (h1 - h0);
            return sea_level_y + offset;
        }
    }
    sea_level_y
}

impl TerrainProfiler {
    pub fn new(config: WorldGenConfig) -> Self {
        Self {
            config,
            climate_sampler: ClimateSampler::new(config),
            hydrology_sampler: HydrologySampler::new(config),
        }
    }

    /// Evaluasi profil terrain lengkap pada titik kontinu $(world\_x, world\_z)$
    pub fn evaluate(&self, world_x: f32, world_z: f32) -> TerrainPoint {
        let climate = self.climate_sampler.sample(world_x, world_z);
        let hydrology = self
            .hydrology_sampler
            .sample(world_x, world_z, climate.continentalness);

        let sea_level_y = self.config.sea_level as f32;

        // 1. Elevasi Dasar Kontinental Berbasis Spline Hermite Mulus (C1 Continuous)
        let base_height = sample_continental_spline(climate.continentalness, sea_level_y);

        // 2. Elevasi Perbukitan dan Pegunungan (Erosion & Peaks Scaling)
        let c = climate.continentalness;
        let mountain_factor = (c.max(0.0) * 1.3).min(1.0);
        let peak_height = (climate.peaks_valleys.max(0.0))
            * (1.0 - climate.erosion.clamp(-0.5, 0.8) * 0.4)
            * self.config.max_mountain_height
            * mountain_factor;

        let hill_height = ((climate.erosion * 0.5 + 0.5) * 8.0) * (c.max(0.05) * 1.5).min(1.0);

        let mut raw_surface_y = base_height + hill_height + peak_height;

        // 3. Pengukiran Lembah Sungai Mulus (Continuous River Carving)
        if hydrology.river_depth > 0.0 {
            let carved_target = sea_level_y - 2.0;
            let carved = raw_surface_y - hydrology.river_depth;
            raw_surface_y = if carved < carved_target {
                carved_target + (carved - carved_target) * 0.2
            } else {
                carved
            };
        }

        // 4. Cekungan Danau Alami Mulus
        if hydrology.lake_dip > 0.0 {
            raw_surface_y -= hydrology.lake_dip;
        }

        // 5. Klasifikasi Biome Berdasarkan Elevasi & Iklim
        let biome = BiomeClassifier::classify(&climate, raw_surface_y, sea_level_y);

        // Water level: minimal sea level pada area lautan/sungai/danau
        let water_level_y = if raw_surface_y < sea_level_y
            || (hydrology.lake_dip > 0.0 && raw_surface_y < sea_level_y + 4.0)
        {
            sea_level_y
        } else if hydrology.is_river && raw_surface_y < base_height {
            (raw_surface_y + 2.0).min(sea_level_y)
        } else {
            sea_level_y
        };

        TerrainPoint {
            surface_height_y: raw_surface_y,
            water_level_y,
            biome,
            climate,
            hydrology,
        }
    }
}
