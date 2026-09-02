use super::config::WorldGenConfig;
use super::noise::{sample_fbm_2d, sample_ridged_2d};
use super::seed::SeedContext;

/// Sampel parameter iklim dan geomorfologi makro pada koordinat global dunia
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimateSample {
    pub continentalness: f32, // [-1.0, 1.0]: Laut Dalam -> Laut -> Pesisir -> Dataran -> Pegunungan
    pub temperature: f32,     // [-1.0, 1.0]: Beku/Salju -> Dingin -> Sedang -> Hangat -> Panas
    pub moisture: f32,        // [-1.0, 1.0]: Gersang/Gurun -> Sedang -> Basah/Hutan
    pub erosion: f32,         // [-1.0, 1.0]: Rata/Erosi Tinggi -> Bergelombang -> Terjal
    pub peaks_valleys: f32,   // [-1.0, 1.0]: Lembah -> Bukit -> Puncak Tinggi
}

/// Sampler iklim global deterministik
pub struct ClimateSampler {
    seeds: SeedContext,
    config: WorldGenConfig,
}

impl ClimateSampler {
    pub fn new(config: WorldGenConfig) -> Self {
        let seeds = SeedContext::new(config.seed, config.generator_version);
        Self { seeds, config }
    }

    /// Evaluasi parameter iklim pada koordinat global kontinu $(world\_x, world\_z)$
    pub fn sample(&self, world_x: f32, world_z: f32) -> ClimateSample {
        // 1. Continentalness: Skala makro besar (low frequency)
        let continentalness = sample_fbm_2d(
            world_x,
            world_z,
            self.seeds.continental_seed,
            4,
            0.5,
            2.0,
            self.config.continental_scale,
        );

        // 2. Temperature: Variasi regional
        let base_temp = sample_fbm_2d(
            world_x,
            world_z,
            self.seeds.temperature_seed,
            3,
            0.5,
            2.0,
            self.config.temperature_scale,
        );

        // 3. Moisture: Variasi kelembaban regional
        let base_moisture = sample_fbm_2d(
            world_x,
            world_z,
            self.seeds.moisture_seed,
            3,
            0.5,
            2.0,
            self.config.moisture_scale,
        );

        // 4. Erosion: Tingkat erosi dan kehalusan lereng
        let erosion = sample_fbm_2d(
            world_x,
            world_z,
            self.seeds.erosion_seed,
            4,
            0.5,
            2.0,
            self.config.erosion_scale,
        );

        // 5. Peaks & Valleys: Puncak pegunungan tajam
        let peaks_valleys = sample_ridged_2d(
            world_x,
            world_z,
            self.seeds.peaks_seed,
            4,
            0.5,
            2.0,
            self.config.erosion_scale * 1.5,
        );

        ClimateSample {
            continentalness,
            temperature: base_temp,
            moisture: base_moisture,
            erosion,
            peaks_valleys: peaks_valleys * 2.0 - 1.0,
        }
    }
}
