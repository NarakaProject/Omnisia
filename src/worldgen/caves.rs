use super::config::WorldGenConfig;
use super::noise::sample_fbm_3d;
use super::seed::SeedContext;

/// Sampler untuk generasi sistem gua 3D volumetrik deterministik
pub struct CaveSampler {
    seeds: SeedContext,
    config: WorldGenConfig,
}

impl CaveSampler {
    pub fn new(config: WorldGenConfig) -> Self {
        let seeds = SeedContext::new(config.seed, config.generator_version);
        Self { seeds, config }
    }

    /// Mengevaluasi apakah titik $(world\_x, world\_y, world\_z)$ merupakan rongga gua (Air)
    /// `surface_y`: Ketinggian permukaan makro pada kolom $(world\_x, world\_z)$
    pub fn is_cave(&self, world_x: f32, world_y: f32, world_z: f32, surface_y: f32) -> bool {
        // Gua tidak pernah mengambang di atas permukaan tanah bebas
        if world_y > surface_y {
            return false;
        }

        let depth_below_surface = surface_y - world_y;

        // 1. Modulasi Kedalaman (Depth Shaping)
        // Di dekat permukaan (kedalaman < 6 voxel), gua ditutup kecuali pada mulut gua yang sangat jarang
        let surface_suppression = if depth_below_surface < 6.0 {
            let t = depth_below_surface / 6.0;
            t * t
        } else {
            1.0
        };

        if surface_suppression <= 0.05 {
            return false;
        }

        let cave_seed1 = self.seeds.river_seed.wrapping_add(1001);
        let cave_seed2 = self.seeds.river_seed.wrapping_add(2002);
        let cheese_seed = self.seeds.river_seed.wrapping_add(3003);

        // 2. Elongated Worm Tunnels (Persilangan dua medan noise 3D kontinu: N1^2 + N2^2 < r^2)
        let worm_scale = self.config.erosion_scale * 1.8;
        let n1 = sample_fbm_3d(
            world_x, world_y, world_z, cave_seed1, 3, 0.5, 2.0, worm_scale,
        );
        let n2 = sample_fbm_3d(
            world_x, world_y, world_z, cave_seed2, 3, 0.5, 2.0, worm_scale,
        );

        let tunnel_dist_sq = n1 * n1 + n2 * n2;
        // Radius tunnel diskalakan dengan kedalaman: lebih sempit dekat permukaan, lebih lebar di kedalaman
        let tunnel_radius_sq = 0.018 * surface_suppression;

        let is_worm_tunnel = tunnel_dist_sq < tunnel_radius_sq;

        // 3. Cheese Caverns (Rongga besar di kedalaman bawah tanah)
        let is_cavern = if depth_below_surface > 25.0 {
            let cavern_scale = self.config.erosion_scale * 1.2;
            let cheese = sample_fbm_3d(
                world_x,
                world_y,
                world_z,
                cheese_seed,
                3,
                0.5,
                2.0,
                cavern_scale,
            );
            cheese > 0.48
        } else {
            false
        };

        is_worm_tunnel || is_cavern
    }
}
