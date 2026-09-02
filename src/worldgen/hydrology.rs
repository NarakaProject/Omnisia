use super::config::WorldGenConfig;
use super::noise::{sample_fbm_2d, sample_gradient_2d};
use super::seed::SeedContext;

/// Hasil evaluasi hidrologi pada koordinat dunia
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HydrologySample {
    pub river_depth: f32, // Kedalaman lembah sungai (0.0 = bukan sungai, > 0.0 = di dalam lembah sungai)
    pub is_river: bool,
    pub lake_dip: f32, // Penurunan cekungan danau kontinu
}

/// Sampler hidrologi global deterministik untuk sungai dan danau
pub struct HydrologySampler {
    seeds: SeedContext,
    config: WorldGenConfig,
}

impl HydrologySampler {
    pub fn new(config: WorldGenConfig) -> Self {
        let seeds = SeedContext::new(config.seed, config.generator_version);
        Self { seeds, config }
    }

    /// Evaluasi kontur sungai dan danau kontinu pada $(world\_x, world\_z)$
    pub fn sample(&self, world_x: f32, world_z: f32, continentalness: f32) -> HydrologySample {
        // Jika berada jauh di tengah lautan dalam, sungai tidak diukir
        if continentalness < -0.3 {
            return HydrologySample {
                river_depth: 0.0,
                is_river: false,
                lake_dip: 0.0,
            };
        }

        // Domain warping halus untuk aliran sungai yang berkelok alami
        let warp_x = sample_gradient_2d(
            world_x * self.config.river_frequency * 0.5,
            world_z * self.config.river_frequency * 0.5,
            self.seeds.river_seed,
        ) * 35.0;
        let warp_z = sample_gradient_2d(
            world_x * self.config.river_frequency * 0.5 + 100.0,
            world_z * self.config.river_frequency * 0.5 + 100.0,
            self.seeds.river_seed.wrapping_add(1),
        ) * 35.0;

        let sample_x = world_x + warp_x;
        let sample_z = world_z + warp_z;

        // Jaringan sungai 2D berbasis nilai kontur kontinu mendekati nol (|noise| < width)
        let river_noise = sample_fbm_2d(
            sample_x,
            sample_z,
            self.seeds.river_seed,
            3,
            0.5,
            2.0,
            self.config.river_frequency,
        );

        let river_dist = river_noise.abs();
        let river_width_threshold = 0.055; // Lebar sungai terkalibrasi

        let (river_depth, is_river) = if river_dist < river_width_threshold {
            // Profil lembah sungai hermite halus (C1 continuous)
            let t = 1.0 - (river_dist / river_width_threshold);
            let smooth_t = t * t * (3.0 - 2.0 * t);
            let depth = smooth_t * 12.0;
            (depth, true)
        } else {
            (0.0, false)
        };

        // Deteksi cekungan danau lokal kontinu menggunakan noise frekuensi sedang
        let lake_noise = sample_fbm_2d(
            world_x,
            world_z,
            self.seeds.river_seed.wrapping_add(777),
            2,
            0.5,
            2.0,
            self.config.river_frequency * 2.0,
        );

        let lake_dip = if lake_noise < -0.55 && continentalness > 0.0 {
            let t = ((-0.55 - lake_noise) / 0.25).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t) * 7.0
        } else {
            0.0
        };

        HydrologySample {
            river_depth,
            is_river,
            lake_dip,
        }
    }
}
