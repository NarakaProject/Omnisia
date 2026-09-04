use glam::Vec3;
use std::f32::consts::{PI, TAU};

/// Evaluates smoothstep for smooth, branchless interpolation.
#[inline]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear interpolation between two 3-component color vectors.
#[inline]
pub fn lerp_vec3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let clamped_t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * clamped_t,
        a[1] + (b[1] - a[1]) * clamped_t,
        a[2] + (b[2] - a[2]) * clamped_t,
    ]
}

/// Evaluated celestial environment parameters derived deterministically from the celestial clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CelestialParameters {
    /// Unit direction pointing FROM the world TOWARDS the sun in the sky.
    pub sun_direction: Vec3,
    /// Sun elevation angle sine `e = sun_direction.y ∈ [-1.0, 1.0]`.
    pub sun_elevation: f32,
    /// Unit direction pointing FROM the world TOWARDS the moon in the sky.
    pub moon_direction: Vec3,
    /// Moon elevation angle sine `moon_direction.y ∈ [-1.0, 1.0]`.
    pub moon_elevation: f32,
    /// Twilight factor in `[0.0, 1.0]`, peaking when the sun is crossing the horizon.
    pub twilight_factor: f32,
    /// Daylight intensity factor in `[0.0, 1.0]`.
    pub day_factor: f32,
    /// Nighttime intensity factor in `[0.0, 1.0]`.
    pub night_factor: f32,
    /// Star field visibility factor in `[0.0, 1.0]` (faded by daylight and twilight).
    pub star_visibility: f32,
    /// Zenith sky color RGB.
    pub zenith_color: [f32; 3],
    /// Horizon sky color RGB.
    pub horizon_color: [f32; 3],
    /// Direct celestial illumination color RGB (sunlight or moonlight).
    pub celestial_light_color: [f32; 3],
    /// Ambient fill illumination color RGB for terrain and atmosphere.
    pub ambient_light_color: [f32; 3],
}

impl CelestialParameters {
    /// Fixed orbital inclination angle of the moon relative to the solar plane (5.0 degrees).
    ///
    /// AMENDMENT 5 COMPLIANCE:
    /// Explicit, deterministic, bounded orbital declination tilt ensuring smooth opposition
    /// without degenerate collinear collisions with the sun.
    pub const MOON_INCLINATION_RAD: f32 = 5.0 * (PI / 180.0);

    /// Evaluates celestial parameters from a canonical day fraction in `[0.0, 1.0)`.
    ///
    /// AMENDMENT 4 COORDINATE CONVENTION COMPLIANCE:
    /// - `+Y` = World Up
    /// - `day_fraction 0.00` -> Midnight -> Sun at `( 0, -1,  0)`
    /// - `day_fraction 0.25` -> Sunrise  -> Sun at `(+1,  0,  0)`
    /// - `day_fraction 0.50` -> Noon     -> Sun at `( 0, +1,  0)`
    /// - `day_fraction 0.75` -> Sunset   -> Sun at `(-1,  0,  0)`
    pub fn evaluate(day_fraction: f32) -> Self {
        let phi = day_fraction.rem_euclid(1.0) * TAU;

        // 1. Sun Direction: circular orbit in the XY plane
        let sun_x = phi.sin();
        let sun_y = -phi.cos();
        let sun_z = 0.0;
        let sun_direction = Vec3::new(sun_x, sun_y, sun_z);
        let sun_elevation = sun_y; // Since sun_direction is a unit vector, y component is elevation sine.

        // 2. Moon Direction: predictable opposition with explicit 5.0 degree declination tilt
        let moon_x = -sun_x;
        let cos_inc = Self::MOON_INCLINATION_RAD.cos();
        let sin_inc = Self::MOON_INCLINATION_RAD.sin();
        let moon_y = -sun_y * cos_inc;
        let moon_z = -sun_y * sin_inc;
        let moon_direction = Vec3::new(moon_x, moon_y, moon_z);
        let moon_elevation = moon_y;

        // 3. Day, Night, and Twilight Factors
        // Day factor ramps smoothly as sun crests the horizon (-0.08 to +0.12)
        let day_factor = smoothstep(-0.08, 0.12, sun_elevation);
        let night_factor = 1.0 - day_factor;

        // Twilight smooth cosine bell curve centered at sun_elevation == 0.0 (horizon crossing)
        let twilight_width = 0.20;
        let dist_from_horizon = sun_elevation.abs();
        let twilight_factor = if dist_from_horizon < twilight_width {
            let t = (dist_from_horizon / twilight_width) * (PI * 0.5);
            t.cos().powi(2)
        } else {
            0.0
        };

        // Star visibility: maximum at deep night, suppressed during day and muted during twilight
        let star_visibility = night_factor * (1.0 - 0.75 * twilight_factor);

        // 4. Harmonic Sky & Horizon Color Palettes
        // Day palette
        let day_zenith = [0.18, 0.42, 0.82];
        let day_horizon = [0.68, 0.82, 0.94];
        let day_sun = [1.0, 0.96, 0.90];
        let day_ambient = [0.45, 0.50, 0.58]; // Matches existing LightUniform default

        // Twilight palette (golden hour / dawn & dusk glow)
        let twi_zenith = [0.12, 0.14, 0.32];
        let twi_horizon = [0.98, 0.50, 0.22];
        let twi_sun = [1.0, 0.65, 0.30];
        let twi_ambient = [0.35, 0.28, 0.32];

        // Night palette (clear starry cosmos and silver moonlight)
        let night_zenith = [0.012, 0.016, 0.035];
        let night_horizon = [0.030, 0.040, 0.080];
        let night_moon = [0.22, 0.28, 0.42];
        let night_ambient = [0.08, 0.10, 0.16];

        // Continuous interpolation across Day, Night, and Twilight
        let base_zenith = lerp_vec3(night_zenith, day_zenith, day_factor);
        let zenith_color = lerp_vec3(base_zenith, twi_zenith, twilight_factor);

        let base_horizon = lerp_vec3(night_horizon, day_horizon, day_factor);
        let horizon_color = lerp_vec3(base_horizon, twi_horizon, twilight_factor);

        let base_light = lerp_vec3(night_moon, day_sun, day_factor);
        let celestial_light_color = lerp_vec3(base_light, twi_sun, twilight_factor);

        let base_ambient = lerp_vec3(night_ambient, day_ambient, day_factor);
        let ambient_light_color = lerp_vec3(base_ambient, twi_ambient, twilight_factor);

        Self {
            sun_direction,
            sun_elevation,
            moon_direction,
            moon_elevation,
            twilight_factor,
            day_factor,
            night_factor,
            star_visibility,
            zenith_color,
            horizon_color,
            celestial_light_color,
            ambient_light_color,
        }
    }
}
