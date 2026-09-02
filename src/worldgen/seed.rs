use serde::{Deserialize, Serialize};

/// Identitas Seed Dunia 64-bit yang deterministik dan stabil lintas-platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldSeed(pub u64);

impl Default for WorldSeed {
    fn default() -> Self {
        Self(1337)
    }
}

impl WorldSeed {
    pub const fn from_u64(seed: u64) -> Self {
        Self(seed)
    }

    /// Mengonversi string seed menjadi u64 seed menggunakan algoritma SplitMix64 deterministik
    pub fn from_string(seed_str: &str) -> Self {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in seed_str.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Self(splitmix64(hash))
    }

    #[inline(always)]
    pub const fn raw(&self) -> u64 {
        self.0
    }
}

/// Versi algoritma Generator Dunia untuk memastikan kompatibilitas save data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GeneratorVersion(pub u32);

impl Default for GeneratorVersion {
    fn default() -> Self {
        Self(1)
    }
}

/// Context turunan seed untuk menghasilkan sub-seed independen untuk tiap medan prosedural
#[derive(Debug, Clone, Copy)]
pub struct SeedContext {
    pub master_seed: u64,
    pub continental_seed: u64,
    pub temperature_seed: u64,
    pub moisture_seed: u64,
    pub erosion_seed: u64,
    pub peaks_seed: u64,
    pub river_seed: u64,
}

impl SeedContext {
    pub fn new(seed: WorldSeed, version: GeneratorVersion) -> Self {
        let master = splitmix64(
            seed.raw()
                .wrapping_add((version.0 as u64).wrapping_mul(0x9E3779B97F4A7C15)),
        );
        Self {
            master_seed: master,
            continental_seed: splitmix64(master.wrapping_add(101)),
            temperature_seed: splitmix64(master.wrapping_add(202)),
            moisture_seed: splitmix64(master.wrapping_add(303)),
            erosion_seed: splitmix64(master.wrapping_add(404)),
            peaks_seed: splitmix64(master.wrapping_add(505)),
            river_seed: splitmix64(master.wrapping_add(606)),
        }
    }
}

#[inline(always)]
pub fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^ (x >> 31)
}
