use super::biome::BiomeType;
use super::config::WorldGenConfig;
use super::noise::{hash3d, sample_fbm_3d};
use super::seed::SeedContext;
use crate::material::MaterialId;

/// Sampler untuk formasi tebing curam dan overhang 3D volumetrik
pub struct OverhangSampler {
    seeds: SeedContext,
    config: WorldGenConfig,
}

impl OverhangSampler {
    pub fn new(config: WorldGenConfig) -> Self {
        let seeds = SeedContext::new(config.seed, config.generator_version);
        Self { seeds, config }
    }

    /// Evaluasi kontribusi densitas 3D overhang pada koordinat $(world\_x, world\_y, world\_z)$
    /// Mengembalikan densitas positif jika terdapat overhang batuan padat yang menggantung di udara
    pub fn sample_density(
        &self,
        world_x: f32,
        world_y: f32,
        world_z: f32,
        surface_y: f32,
        biome: BiomeType,
    ) -> f32 {
        // Overhang hanya aktif pada biome pegunungan, perbukitan, atau tebing pesisir
        let biome_weight = match biome {
            BiomeType::Mountains | BiomeType::SnowPeaks => 1.2,
            BiomeType::Hills => 0.8,
            BiomeType::Beach | BiomeType::Forest => 0.4,
            _ => 0.0,
        };

        if biome_weight <= 0.0 {
            return 0.0;
        }

        // Overhang beroperasi pada rentang vertikal di sekitar garis permukaan lereng
        let dy = world_y - surface_y;
        if dy < -12.0 || dy > 16.0 {
            return 0.0;
        }

        let overhang_seed = self.seeds.erosion_seed.wrapping_add(4004);
        let scale = self.config.erosion_scale * 1.5;

        // Sampling noise 3D untuk distorsi volumetrik
        let n3d = sample_fbm_3d(
            world_x,
            world_y * 0.8,
            world_z,
            overhang_seed,
            3,
            0.5,
            2.0,
            scale,
        );

        // Faktor bentuk vertikal: puncak overhang menggantung di atas udara
        let height_factor = if dy > 0.0 {
            (1.0 - (dy / 16.0)).max(0.0)
        } else {
            (1.0 + (dy / 12.0)).max(0.0)
        };

        if n3d > 0.25 {
            (n3d - 0.25) * 6.0 * height_factor * biome_weight
        } else {
            0.0
        }
    }
}

/// Identifikasi stratifikasi lapisan geologi bawah tanah
pub struct UndergroundStrata;

impl UndergroundStrata {
    /// Menentukan tipe material dasar solid berdasarkan kedalaman dan ketinggian dunia
    pub fn determine_base_material(
        world_y: i32,
        surface_y: i32,
        biome: BiomeType,
        mats: &crate::worldgen::voxelizer::ResolvedGenMaterials,
    ) -> MaterialId {
        if world_y >= surface_y {
            // Lapisan Permukaan Teratas (Topsoil)
            match biome {
                BiomeType::SnowPeaks => mats.snow,
                BiomeType::Mountains => {
                    if world_y > 45 {
                        mats.snow
                    } else {
                        mats.stone
                    }
                }
                BiomeType::Desert | BiomeType::Beach | BiomeType::Ocean | BiomeType::DeepOcean => {
                    mats.sand
                }
                _ => mats.grass,
            }
        } else if world_y >= surface_y - 4 {
            // Lapisan Subpermukaan (Subsoil)
            match biome {
                BiomeType::Desert | BiomeType::Beach => mats.sand,
                BiomeType::SnowPeaks | BiomeType::Mountains => mats.stone,
                _ => mats.dirt,
            }
        } else if world_y >= -32 {
            // Lapisan Batu Atas (Upper Strata Stone)
            mats.stone
        } else {
            // Lapisan Batu Dalam (Deep Strata Deepslate)
            mats.deepslate
        }
    }
}

/// Sampler untuk distribusi urat/kantong bijih mineral deterministik
pub struct OreSampler {
    seed: u64,
}

impl OreSampler {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// Evaluasi apakah titik solid digantikan oleh bijih mineral (Coal, Iron, Gold, Crystal)
    /// Hard invariant: Fungsi ini HANYA dipanggil jika voxel asal adalah batu/solid dasar
    pub fn sample_ore(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        mats: &crate::worldgen::voxelizer::ResolvedGenMaterials,
    ) -> Option<MaterialId> {
        let h = hash3d(world_x, world_y, world_z, self.seed);

        // 1. Coal Ore: Sering ditemukan di lapisan atas ($y \in [-16, 64]$)
        if (-16..=64).contains(&world_y) {
            let cell_x = world_x.div_euclid(6);
            let cell_y = world_y.div_euclid(6);
            let cell_z = world_z.div_euclid(6);
            let cell_hash = hash3d(cell_x, cell_y, cell_z, self.seed.wrapping_add(101));
            if cell_hash % 100 < 10 {
                let center_offset_x = (cell_hash % 5) as i32 + 1;
                let center_offset_y = ((cell_hash >> 8) % 5) as i32 + 1;
                let center_offset_z = ((cell_hash >> 16) % 5) as i32 + 1;

                let local_x = world_x.rem_euclid(6);
                let local_y = world_y.rem_euclid(6);
                let local_z = world_z.rem_euclid(6);

                let dist_sq = (local_x - center_offset_x).pow(2)
                    + (local_y - center_offset_y).pow(2)
                    + (local_z - center_offset_z).pow(2);

                if dist_sq <= 3 {
                    return Some(mats.coal_ore);
                }
            }
        }

        // 2. Iron Ore: Lapisan menengah ($y \in [-48, 24]$)
        if (-48..=24).contains(&world_y) {
            let cell_x = world_x.div_euclid(8);
            let cell_y = world_y.div_euclid(8);
            let cell_z = world_z.div_euclid(8);
            let cell_hash = hash3d(cell_x, cell_y, cell_z, self.seed.wrapping_add(202));
            if cell_hash % 100 < 8 {
                let center_offset_x = (cell_hash % 7) as i32 + 1;
                let center_offset_y = ((cell_hash >> 8) % 7) as i32 + 1;
                let center_offset_z = ((cell_hash >> 16) % 7) as i32 + 1;

                let local_x = world_x.rem_euclid(8);
                let local_y = world_y.rem_euclid(8);
                let local_z = world_z.rem_euclid(8);

                let dist_sq = (local_x - center_offset_x).pow(2)
                    + (local_y - center_offset_y).pow(2)
                    + (local_z - center_offset_z).pow(2);

                if dist_sq <= 4 {
                    return Some(mats.iron_ore);
                }
            }
        }

        // 3. Gold Ore: Lapisan dalam ($y \le -10$)
        if world_y <= -10 {
            let cell_x = world_x.div_euclid(10);
            let cell_y = world_y.div_euclid(10);
            let cell_z = world_z.div_euclid(10);
            let cell_hash = hash3d(cell_x, cell_y, cell_z, self.seed.wrapping_add(303));
            if cell_hash % 100 < 5 {
                let center_offset_x = (cell_hash % 9) as i32 + 1;
                let center_offset_y = ((cell_hash >> 8) % 9) as i32 + 1;
                let center_offset_z = ((cell_hash >> 16) % 9) as i32 + 1;

                let local_x = world_x.rem_euclid(10);
                let local_y = world_y.rem_euclid(10);
                let local_z = world_z.rem_euclid(10);

                let dist_sq = (local_x - center_offset_x).pow(2)
                    + (local_y - center_offset_y).pow(2)
                    + (local_z - center_offset_z).pow(2);

                if dist_sq <= 3 {
                    return Some(mats.gold_ore);
                }
            }
        }

        // 4. Lumina Crystal: Kantong langka di kedalaman ($y \le -32$)
        if world_y <= -32 && h % 1000 < 6 {
            return Some(mats.crystal);
        }

        None
    }
}

/// Sampler untuk formasi batuan alami (surface boulders & rock outcroppings)
pub struct FormationSampler {
    seed: u64,
}

impl FormationSampler {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// Evaluasi apakah terdapat formasi batuan alami menonjol di atas permukaan tanah
    pub fn sample_surface_formation(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        surface_y: i32,
        biome: BiomeType,
        mats: &crate::worldgen::voxelizer::ResolvedGenMaterials,
    ) -> Option<MaterialId> {
        let dy = world_y - surface_y;
        if dy <= 0 || dy > 3 {
            return None;
        }

        match biome {
            BiomeType::Mountains | BiomeType::Hills | BiomeType::Forest | BiomeType::Plains => {
                let cell_x = world_x.div_euclid(16);
                let cell_z = world_z.div_euclid(16);
                let cell_hash = hash3d(cell_x, 0, cell_z, self.seed.wrapping_add(505));

                if cell_hash % 100 < 12 {
                    let cx = (cell_hash % 13) as i32 + 2;
                    let cz = ((cell_hash >> 8) % 13) as i32 + 2;

                    let lx = world_x.rem_euclid(16);
                    let lz = world_z.rem_euclid(16);

                    let dist_sq = (lx - cx).pow(2) + (lz - cz).pow(2);
                    let boulder_radius_sq = 4 - dy;

                    if dist_sq <= boulder_radius_sq {
                        return Some(mats.stone);
                    }
                }
            }
            _ => {}
        }

        None
    }
}
