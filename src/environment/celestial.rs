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
    /// Unit direction pointing FROM the world TOWARDS the active celestial light source (sun in day, moon at night).
    pub celestial_light_direction: Vec3,
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
    /// Visual sunlight radiance RGB for sky disc and daytime illumination.
    pub sun_color: [f32; 3],
    /// Direct celestial illumination color RGB for terrain (sunlight by day, subtle cool moonlight by night).
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
    /// - `day_fraction 0.00` -> Midnight -> Sun at `( 0, -1,  0)`, Moon in zenith sky
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
        let star_visibility = night_factor * (1.0 - 0.80 * twilight_factor);

        // 4. Harmonic Sky & Horizon Color Palettes
        // Day palette (Preserves existing daytime lighting, Mandates 3 & 18)
        let day_zenith = [0.18, 0.42, 0.82];
        let day_horizon = [0.68, 0.82, 0.94];
        let day_sun = [1.0, 0.96, 0.90];
        let day_ambient = [0.45, 0.50, 0.58]; // Matches existing LightUniform default

        // Twilight palette (golden hour / dawn & dusk glow)
        let twi_zenith = [0.12, 0.14, 0.32];
        let twi_horizon = [0.98, 0.50, 0.22];
        let twi_sun = [1.0, 0.65, 0.30];
        let twi_ambient = [0.35, 0.28, 0.32];

        // Dark natural night palette (Mandates 4 & 6)
        let night_zenith = [0.012, 0.016, 0.035];
        let night_horizon = [0.025, 0.032, 0.060];
        // Subtle cool directional moonlight on terrain (Mandate 6: subtle, not washed out)
        let night_moon = [0.035, 0.050, 0.080];
        // Dark natural night ambient floor (Mandate 4: deep shadow genuinely dark)
        let night_ambient = [0.015, 0.020, 0.032];

        // 5. Sky Colors: Continuous interpolation across Day, Night, and Twilight
        let base_zenith = lerp_vec3(night_zenith, day_zenith, day_factor);
        let zenith_color = lerp_vec3(base_zenith, twi_zenith, twilight_factor);

        let base_horizon = lerp_vec3(night_horizon, day_horizon, day_factor);
        let horizon_color = lerp_vec3(base_horizon, twi_horizon, twilight_factor);

        let base_ambient = lerp_vec3(night_ambient, day_ambient, day_factor);
        let ambient_light_color = lerp_vec3(base_ambient, twi_ambient, twilight_factor);

        // Visual sun radiance for sky shader (warm in twilight, bright in day)
        let sun_color = lerp_vec3(day_sun, twi_sun, twilight_factor);

        // 6. Independent Sun and Moon Direct Light Contribution Weights (Mandates 2, 6, 7)
        // Sunlight fades smoothly as sun descends towards the horizon (-0.06 to +0.08)
        let sun_weight = smoothstep(-0.06, 0.08, sun_elevation);
        // Moonlight fades in as sun sets and moon rises above the horizon
        let moon_weight =
            smoothstep(0.04, -0.08, sun_elevation) * smoothstep(-0.02, 0.08, moon_elevation);

        // Active celestial light source selection:
        // When sun direct weight is dominant, sun is the celestial source;
        // when moon direct weight is dominant, moon is the celestial source.
        // At crossover (sun_elevation ≈ -0.02), both direct weights are near zero,
        // so transitioning vectors produces ZERO visual jump or pop!
        let (celestial_light_direction, celestial_light_color) = if sun_weight >= moon_weight {
            let color = [
                sun_color[0] * sun_weight,
                sun_color[1] * sun_weight,
                sun_color[2] * sun_weight,
            ];
            (sun_direction, color)
        } else {
            let color = [
                night_moon[0] * moon_weight,
                night_moon[1] * moon_weight,
                night_moon[2] * moon_weight,
            ];
            (moon_direction, color)
        };

        Self {
            sun_direction,
            sun_elevation,
            moon_direction,
            moon_elevation,
            celestial_light_direction,
            twilight_factor,
            day_factor,
            night_factor,
            star_visibility,
            zenith_color,
            horizon_color,
            sun_color,
            celestial_light_color,
            ambient_light_color,
        }
    }
}

/// Deterministic CPU reference evaluation of the procedural star field (Mandates 10 & 11).
///
/// This is a deterministic testing reference model that evaluates procedural star generation
/// using the same spatial 3D hashing algorithm as `sky.wgsl`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarReferenceResult {
    /// 3D integer cell coordinate on the star sphere.
    pub cell: [i32; 3],
    /// Whether this celestial cell is an active star cell (rnd.x > 0.975).
    pub is_star_cell: bool,
    /// Whether this celestial direction falls within the star point radius.
    pub is_star: bool,
    /// Computed star brightness factor before daylight suppression.
    pub base_brightness: f32,
    /// Effective star brightness taking twinkle and daylight suppression into account.
    pub effective_brightness: f32,
}

#[inline]
pub fn fract_f32(x: f32) -> f32 {
    x - x.floor()
}

#[inline]
pub fn hash33_cpu(p: Vec3) -> Vec3 {
    let mut p3 = Vec3::new(
        fract_f32(p.x * 0.1031),
        fract_f32(p.y * 0.1030),
        fract_f32(p.z * 0.0973),
    );
    let d = p3.dot(Vec3::new(p3.y, p3.x, p3.z) + 33.33);
    p3 += Vec3::splat(d);
    Vec3::new(
        fract_f32((p3.x + p3.y) * p3.z),
        fract_f32((p3.x + p3.x) * p3.y),
        fract_f32((p3.y + p3.x) * p3.x),
    )
}

pub fn evaluate_star_reference(
    dir: Vec3,
    bounded_time: f32,
    star_visibility: f32,
) -> StarReferenceResult {
    let norm_dir = dir.normalize();
    let star_grid = norm_dir * 140.0;
    let cell_vec = star_grid.floor();
    let cell = [cell_vec.x as i32, cell_vec.y as i32, cell_vec.z as i32];
    let frac_coord = star_grid - cell_vec;
    let rnd = hash33_cpu(cell_vec);

    let is_star_cell = rnd.x > 0.975 && norm_dir.y > -0.02;
    if !is_star_cell || star_visibility < 0.005 {
        return StarReferenceResult {
            cell,
            is_star_cell,
            is_star: false,
            base_brightness: 0.0,
            effective_brightness: 0.0,
        };
    }

    let star_center = rnd * 0.6 + Vec3::splat(0.2);
    let dist = (frac_coord - star_center).length();
    let star_radius = 0.20;

    let star_point = if dist < star_radius {
        let t = ((dist - star_radius) / (0.03 - star_radius)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    } else {
        0.0
    };

    let horizon_fade = smoothstep(-0.02, 0.06, norm_dir.y);
    let twinkle = 0.75 + 0.25 * (bounded_time * 3.5 + rnd.y * 62.83).sin();
    let magnitude = rnd.y.powi(3) * 2.5 + 0.8;

    let base_brightness = star_point * magnitude * horizon_fade;
    let effective_brightness = base_brightness * twinkle * star_visibility;

    StarReferenceResult {
        cell,
        is_star_cell: true,
        is_star: star_point > 0.01,
        base_brightness,
        effective_brightness,
    }
}
