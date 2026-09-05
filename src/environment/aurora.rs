use super::celestial::smoothstep;

/// Configuration and evaluated visual parameters for the procedural aurora layer.
///
/// ARCHITECTURAL FIREWALL:
/// Aurora is strictly an environmental visual layer rendered in the sky pass (`sky.wgsl`).
/// It is NOT a weather simulation: it does not own weather states, wind, precipitation,
/// fluid simulations, cloud systems, or entities. It does NOT illuminate terrain and does NOT
/// modify `LightUniform`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuroraParameters {
    /// Visual intensity multiplier in `[0.0, 10.0]`. Default is `1.0`.
    /// Setting this to `0.0` disables aurora rendering.
    pub intensity: f32,
}

impl Default for AuroraParameters {
    fn default() -> Self {
        Self { intensity: 1.0 }
    }
}

impl AuroraParameters {
    /// Maximum allowed aurora intensity multiplier.
    pub const MAX_INTENSITY: f32 = 10.0;

    /// Creates an aurora configuration with default intensity (`1.0`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates the smooth night visibility factor for the aurora based on celestial sun elevation.
    ///
    /// MANDATORY AMENDMENT A COMPLIANCE:
    /// Uses strictly ascending edges in smoothstep:
    /// `1.0 - smoothstep(-0.18, -0.06, sun_elevation)`
    ///
    /// - `sun_elevation >= -0.06` (Day, Sunset, Early Civil Dusk): `0.0`
    /// - `sun_elevation ∈ (-0.18, -0.06)` (Nautical Dusk / Dawn): smoothly transitions between `0.0` and `1.0`
    /// - `sun_elevation <= -0.18` (Deep Night, Midnight): `1.0`
    #[inline]
    pub fn visibility(sun_elevation: f32) -> f32 {
        1.0 - smoothstep(-0.18, -0.06, sun_elevation)
    }

    /// Sets the visual intensity multiplier within the safe, bounded range `[0.0, 10.0]`.
    pub fn set_intensity(&mut self, intensity: f32) -> Result<(), &'static str> {
        if !intensity.is_finite() {
            return Err("intensity must be a finite number");
        }
        if intensity < 0.0 {
            return Err("intensity cannot be negative");
        }
        if intensity > Self::MAX_INTENSITY {
            return Err("intensity exceeds maximum allowed bound of 10.0");
        }
        self.intensity = intensity;
        Ok(())
    }

    /// Evaluates the effective CPU reference aurora emission factor for deterministic testing.
    ///
    /// Returns `intensity * visibility(sun_elevation)`.
    #[inline]
    pub fn effective_emission(&self, sun_elevation: f32) -> f32 {
        self.intensity * Self::visibility(sun_elevation)
    }
}

/// Deterministic CPU reference evaluation of the procedural aurora curtain coordinate model.
///
/// PHASE 10.6.1 COMPLIANCE:
/// The primary aurora curtain arc is anchored along the `-Z` world axis.
/// Given a camera position `camera_pos` and celestial view direction `dir`:
/// 1. Computes geometric intersections with three atmospheric layer shells (Far, Main, Fine).
/// 2. Returns the spatial main layer coordinates `(P_x, P_z)` and layer ray distances `(t_far, t_main, t_fine)`.
/// 3. Returns the directional envelope factor ensuring the curtain fades smoothly near the horizon
///    and upper dome.
/// 4. Evaluates world anchor alignment with strictly ordered smoothstep edges (`1.0 - smoothstep(-0.55, 0.25, dir.z)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuroraReferenceResult {
    /// Spatial layer X coordinate on the main atmospheric layer.
    pub layer_x: f32,
    /// Spatial layer Z coordinate on the main atmospheric layer.
    pub layer_z: f32,
    /// Ray intersection distance to the Far atmospheric layer (2400m base).
    pub t_far: f32,
    /// Ray intersection distance to the Main atmospheric layer (1500m base).
    pub t_main: f32,
    /// Ray intersection distance to the Fine atmospheric layer (1050m base).
    pub t_fine: f32,
    /// Vertical atmospheric envelope in `[0.0, 1.0]`.
    pub vertical_envelope: f32,
    /// World anchor directional alignment towards `-Z` in `[0.0, 1.0]`.
    pub anchor_alignment: f32,
    /// Effective emission factor in `[0.0, 10.0]`.
    pub effective_emission: f32,
}

/// Evaluates the deterministic reference aurora model on the CPU.
pub fn evaluate_aurora_reference(
    camera_pos: glam::Vec3,
    dir: glam::Vec3,
    sun_elevation: f32,
    intensity: f32,
) -> AuroraReferenceResult {
    let effective_emission = AuroraParameters { intensity }.effective_emission(sun_elevation);

    // Altitude stability guard: clamps camera Y to [-100.0, 4000.0] to prevent layer collapse,
    // coordinate singularities, and negative effective heights at extreme altitudes (e.g. Y=5000m).
    let clamped_cam_y = camera_pos.y.clamp(-100.0, 4000.0);

    // Effective layer heights for the three apparent spatial depths:
    let h_far = (2400.0 - clamped_cam_y * 0.15).max(400.0);
    let h_main = (1500.0 - clamped_cam_y * 0.20).max(400.0);
    let h_fine = (1050.0 - clamped_cam_y * 0.25).max(400.0);

    // Distance to atmospheric layer planes along view direction:
    let safe_dy = dir.y.max(0.04);
    let t_far = h_far / safe_dy;
    let t_main = h_main / safe_dy;
    let t_fine = h_fine / safe_dy;

    // Spatial main layer position in world space:
    let layer_x = camera_pos.x + t_main * dir.x;
    let layer_z = camera_pos.z + t_main * dir.z;

    // Vertical envelope: fades near horizon (dir.y < 0.04) and towards zenith (dir.y > 0.60):
    let horizon_fade = smoothstep(0.04, 0.16, dir.y);
    let zenith_fade = 1.0 - smoothstep(0.60, 0.88, dir.y);
    let vertical_envelope = horizon_fade * zenith_fade;

    // World anchor alignment: centered along the -Z world axis (Amendment C & Section 16).
    // Uses strictly ordered edges: -0.55 <= 0.25. Full alignment for dir.z <= -0.55; zero for dir.z >= 0.25.
    let anchor_alignment = 1.0 - smoothstep(-0.55, 0.25, dir.z);

    AuroraReferenceResult {
        layer_x,
        layer_z,
        t_far,
        t_main,
        t_fine,
        vertical_envelope,
        anchor_alignment,
        effective_emission,
    }
}
