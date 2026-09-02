use serde::{Deserialize, Serialize};

use super::seed::{splitmix64, GeneratorVersion, WorldSeed};

/// Konfigurasi parameter generasi dunia Omnisia
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldGenConfig {
    pub seed: WorldSeed,
    pub generator_version: GeneratorVersion,
    pub sea_level: i32,
    pub continental_scale: f32,
    pub erosion_scale: f32,
    pub temperature_scale: f32,
    pub moisture_scale: f32,
    pub river_frequency: f32,
    pub max_mountain_height: f32,
}

impl Default for WorldGenConfig {
    fn default() -> Self {
        Self {
            seed: WorldSeed::default(),
            generator_version: GeneratorVersion::default(),
            sea_level: 16, // World Y = 16 voxel (8.0 meter)
            continental_scale: 0.002,
            erosion_scale: 0.005,
            temperature_scale: 0.0015,
            moisture_scale: 0.0015,
            river_frequency: 0.0035,
            max_mountain_height: 56.0,
        }
    }
}

impl WorldGenConfig {
    pub fn new(seed: WorldSeed) -> Self {
        Self {
            seed,
            ..Default::default()
        }
    }

    /// Menghasilkan 64-bit hash konfigurasi untuk identitas dunia
    pub fn config_hash(&self) -> u64 {
        let mut h = self.seed.raw();
        h = splitmix64(h.wrapping_add(self.generator_version.0 as u64));
        h = splitmix64(h.wrapping_add(self.sea_level as u64));
        h = splitmix64(h.wrapping_add((self.continental_scale.to_bits()) as u64));
        h = splitmix64(h.wrapping_add((self.erosion_scale.to_bits()) as u64));
        h = splitmix64(h.wrapping_add((self.temperature_scale.to_bits()) as u64));
        h = splitmix64(h.wrapping_add((self.moisture_scale.to_bits()) as u64));
        h = splitmix64(h.wrapping_add((self.river_frequency.to_bits()) as u64));
        h = splitmix64(h.wrapping_add((self.max_mountain_height.to_bits()) as u64));
        h
    }

    pub fn identity(&self) -> WorldIdentity {
        WorldIdentity {
            seed: self.seed,
            version: self.generator_version,
            config_hash: self.config_hash(),
        }
    }
}

/// Identitas struktural dunia yang membedakan versi dan konfigurasi generator secara eksplisit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldIdentity {
    pub seed: WorldSeed,
    pub version: GeneratorVersion,
    pub config_hash: u64,
}
