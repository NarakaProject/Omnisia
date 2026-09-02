use serde::{Deserialize, Serialize};

use super::climate::ClimateSample;

/// Tipe biome alam yang dihasilkan oleh generator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BiomeType {
    DeepOcean,
    Ocean,
    Beach,
    Plains,
    Forest,
    Desert,
    Hills,
    Mountains,
    SnowPeaks,
}

impl BiomeType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::DeepOcean => "Deep Ocean",
            Self::Ocean => "Ocean",
            Self::Beach => "Beach",
            Self::Plains => "Plains",
            Self::Forest => "Forest",
            Self::Desert => "Desert",
            Self::Hills => "Hills",
            Self::Mountains => "Mountains",
            Self::SnowPeaks => "Snow Peaks",
        }
    }

    pub fn is_ocean(&self) -> bool {
        matches!(self, Self::DeepOcean | Self::Ocean)
    }
}

/// Pengklasifikasi biome deterministik berbasis sampel iklim dan benua
pub struct BiomeClassifier;

impl BiomeClassifier {
    pub fn classify(climate: &ClimateSample, elevation_y: f32, sea_level_y: f32) -> BiomeType {
        let c = climate.continentalness;
        let t = climate.temperature;
        let m = climate.moisture;

        // 1. Zona Lautan Makro (Continentalness rendah)
        if c < -0.45 {
            return BiomeType::DeepOcean;
        }
        if c < -0.12 {
            return BiomeType::Ocean;
        }

        // 2. Zona Pesisir Pantai (Perbatasan daratan & lautan)
        if c < 0.05 || (elevation_y <= sea_level_y + 3.0 && c < 0.2) {
            return BiomeType::Beach;
        }

        // 3. Puncak Gunung Sangat Tinggi / Bersalju (Suhu dingin atau elevasi sangat tinggi)
        if elevation_y > sea_level_y + 36.0 || (elevation_y > sea_level_y + 24.0 && t < -0.2) {
            return BiomeType::SnowPeaks;
        }

        // 4. Pegunungan Tinggi
        if c > 0.6 || elevation_y > sea_level_y + 20.0 {
            return BiomeType::Mountains;
        }

        // 5. Perbukitan
        if c > 0.35 || climate.peaks_valleys > 0.3 {
            return BiomeType::Hills;
        }

        // 6. Dataran Rendah Daratan Berdasarkan Suhu & Kelembaban
        if t > 0.3 && m < -0.2 {
            BiomeType::Desert
        } else if m > 0.15 {
            BiomeType::Forest
        } else {
            BiomeType::Plains
        }
    }
}
