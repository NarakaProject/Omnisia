use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use super::celestial::CelestialParameters;
use super::time::EnvironmentClock;
use crate::renderer::LightUniform;

/// GPU uniform structure for the procedural sky pass.
///
/// WGSL ALIGNMENT (std140 compliant):
/// Total size: 176 bytes (11 x 16-byte vectors).
/// Every 3-component vector is bundled with an f32 scalar to guarantee 16-byte stride
/// without implicit padding gaps or misalignment across graphics drivers.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SkyUniform {
    /// Inverse View-Projection matrix to unproject clip-space coordinates to world-space directions.
    pub inv_view_proj: [f32; 16],
    /// World camera position.
    pub camera_pos: [f32; 3],
    /// Bounded cyclic animation time in `[0.0, 60.0)` for star twinkle without float precision loss.
    pub bounded_time: f32,
    /// Unit direction pointing FROM the world TOWARDS the sun.
    pub sun_direction: [f32; 3],
    /// Sun elevation sine `e ∈ [-1.0, 1.0]`.
    pub sun_elevation: f32,
    /// Unit direction pointing FROM the world TOWARDS the moon.
    pub moon_direction: [f32; 3],
    /// Continuous moon phase `∈ [0.0, 1.0)`.
    pub moon_phase: f32,
    /// Direct celestial illumination color RGB.
    pub sun_color: [f32; 3],
    /// Twilight factor in `[0.0, 1.0]`.
    pub twilight_factor: f32,
    /// Ambient illumination color RGB.
    pub ambient_color: [f32; 3],
    /// Star field visibility factor in `[0.0, 1.0]`.
    pub star_visibility: f32,
    /// Horizon atmospheric color RGB.
    pub horizon_color: [f32; 3],
    /// Daylight factor in `[0.0, 1.0]`.
    pub day_factor: f32,
    /// Zenith sky color RGB.
    pub zenith_color: [f32; 3],
    /// Explicit padding to ensure 16-byte uniform alignment.
    pub _pad0: f32,
}

impl Default for SkyUniform {
    fn default() -> Self {
        let default_celestial = CelestialParameters::evaluate(0.25);
        Self {
            inv_view_proj: Mat4::IDENTITY.to_cols_array(),
            camera_pos: [0.0; 3],
            bounded_time: 0.0,
            sun_direction: default_celestial.sun_direction.to_array(),
            sun_elevation: default_celestial.sun_elevation,
            moon_direction: default_celestial.moon_direction.to_array(),
            moon_phase: 0.5,
            sun_color: default_celestial.celestial_light_color,
            twilight_factor: default_celestial.twilight_factor,
            ambient_color: default_celestial.ambient_light_color,
            star_visibility: default_celestial.star_visibility,
            horizon_color: default_celestial.horizon_color,
            day_factor: default_celestial.day_factor,
            zenith_color: default_celestial.zenith_color,
            _pad0: 0.0,
        }
    }
}

/// Authoritative coordinator of derived visual environment parameters.
///
/// ARCHITECTURAL FIREWALL:
/// `EnvironmentState` is a derived visual environment model.
/// It NEVER mutates `ChunkStore`, `PhysicsRuntime`, `StructuralSystem`, or CSG transactions.
/// Its outputs strictly drive `SkyUniform` (sky pass) and `LightUniform` (terrain lighting).
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentState {
    /// Deterministic celestial clock.
    pub clock: EnvironmentClock,
    /// Evaluated celestial positions, elevations, and atmospheric colors.
    pub celestial: CelestialParameters,
}

impl Default for EnvironmentState {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvironmentState {
    /// Creates a default environment state initialized to sunrise (0.25 = 06:00).
    pub fn new() -> Self {
        let clock = EnvironmentClock::default();
        let celestial = CelestialParameters::evaluate(clock.day_fraction);
        Self { clock, celestial }
    }

    /// Creates an environment state initialized with a custom initial day fraction and cycle length.
    pub fn with_clock(clock: EnvironmentClock) -> Self {
        let celestial = CelestialParameters::evaluate(clock.day_fraction);
        Self { clock, celestial }
    }

    /// Advances the environment time by `dt_secs` and re-evaluates all celestial parameters.
    pub fn advance(&mut self, dt_secs: f32) {
        self.clock.advance(dt_secs);
        self.celestial = CelestialParameters::evaluate(self.clock.day_fraction);
    }

    /// Freezes environment time progression (Amendment 1 & 8).
    #[inline]
    pub fn pause(&mut self) {
        self.clock.pause();
    }

    /// Resumes environment time progression.
    #[inline]
    pub fn resume(&mut self) {
        self.clock.resume();
    }

    /// Returns whether environment time progression is paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.clock.is_paused()
    }

    /// Sets the time progression scale on the authoritative clock.
    #[inline]
    pub fn set_time_scale(&mut self, scale: f32) -> Result<(), &'static str> {
        self.clock.set_time_scale(scale)
    }

    /// Sets the normalized day fraction directly in `[0.0, 1.0)` and updates celestial parameters.
    pub fn set_day_fraction(&mut self, fraction: f32) {
        self.clock.set_day_fraction(fraction);
        self.celestial = CelestialParameters::evaluate(self.clock.day_fraction);
    }

    /// Builds the GPU-ready `SkyUniform` representation.
    pub fn build_sky_uniform(&self, inv_view_proj: Mat4, camera_pos: Vec3) -> SkyUniform {
        SkyUniform {
            inv_view_proj: inv_view_proj.to_cols_array(),
            camera_pos: camera_pos.to_array(),
            bounded_time: self.clock.bounded_star_time(),
            sun_direction: self.celestial.sun_direction.to_array(),
            sun_elevation: self.celestial.sun_elevation,
            moon_direction: self.celestial.moon_direction.to_array(),
            moon_phase: self.clock.moon_phase(),
            sun_color: self.celestial.celestial_light_color,
            twilight_factor: self.celestial.twilight_factor,
            ambient_color: self.celestial.ambient_light_color,
            star_visibility: self.celestial.star_visibility,
            horizon_color: self.celestial.horizon_color,
            day_factor: self.celestial.day_factor,
            zenith_color: self.celestial.zenith_color,
            _pad0: 0.0,
        }
    }

    /// Builds the harmonized `LightUniform` for the existing terrain renderer.
    ///
    /// The sunlight ray vector is the opposite of the sun's sky direction (`-sun_direction`),
    /// ensuring that `L = normalize(-light.sun_direction)` in `shader.wgsl` points towards the sun.
    pub fn build_light_uniform(&self) -> LightUniform {
        let sunlight_direction = -self.celestial.sun_direction;
        LightUniform {
            sun_direction: sunlight_direction.to_array(),
            _pad1: 0.0,
            sun_color: self.celestial.celestial_light_color,
            _pad2: 0.0,
            ambient_color: self.celestial.ambient_light_color,
            _pad3: 0.0,
        }
    }
}
