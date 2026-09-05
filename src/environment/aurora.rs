use super::celestial::smoothstep;

/// Canonical procedural color presets for the aurora visual layer.
///
/// PHASE 10.6.1R COMPLIANCE:
/// Palettes affect chromatic output strictly after scalar morphology evaluation.
/// All presets share the exact same spatial geometry, folds, filaments, and depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum AuroraPaletteId {
    /// Legacy emerald base with turquoise folds, bright cyan filaments, and soft violet upper tips.
    #[default]
    Default = 0,
    /// Authentic Earth-like aurora during active storms: Bright Emerald oxygen body,
    /// Magenta/Hot Pink lower nitrogen fringe, Pale Mint filaments, and Deep Crimson upper glow.
    ClassicGeomagneticStorm = 1,
    /// Rare high-altitude red aurora: Deep Wine body, Rich Crimson folds, Coral Pink filaments,
    /// and Faint Lavender upper fringe.
    HighAltitudeCrimson = 2,
    /// Twilight / dawn nitrogen-rich character: Electric Blue base, Cobalt Violet folds,
    /// Neon Orchid filaments, and Soft Rose upper flare.
    PolarVioletDawn = 3,
    /// Atmospheric visual preset inspired by STEVE-like sub-auroral arcs:
    /// Faint Sage green fringe, Mauve arc body, Bright Lilac core filaments, and Smoky Indigo diffusion.
    /// (Visual atmospheric inspiration, NOT a dedicated physical simulation).
    GhostlySteve = 4,
    /// Quiet night with low solar activity: Forest Teal base, Spring Apple Green folds,
    /// Pale Seafoam filaments, and Faint Warm Amber upper glow.
    DeepArcticCalm = 5,
}

impl AuroraPaletteId {
    /// Number of available palettes.
    pub const COUNT: usize = 6;

    /// Converts a raw u32 to the corresponding palette, clamping invalid values to Default.
    pub fn from_u32(val: u32) -> Self {
        match val {
            1 => Self::ClassicGeomagneticStorm,
            2 => Self::HighAltitudeCrimson,
            3 => Self::PolarVioletDawn,
            4 => Self::GhostlySteve,
            5 => Self::DeepArcticCalm,
            _ => Self::Default,
        }
    }

    /// Canonical display and identifier name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Default => "Default (Cyan/Violet)",
            Self::ClassicGeomagneticStorm => "Classic Geomagnetic Storm",
            Self::HighAltitudeCrimson => "High-Altitude Crimson Curtain",
            Self::PolarVioletDawn => "Polar Violet Dawn",
            Self::GhostlySteve => "Ghostly STEVE (Sub-Auroral Arc)",
            Self::DeepArcticCalm => "Deep Arctic Calm",
        }
    }

    /// Short command-line identifier.
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::ClassicGeomagneticStorm => "storm",
            Self::HighAltitudeCrimson => "crimson",
            Self::PolarVioletDawn => "violet",
            Self::GhostlySteve => "steve",
            Self::DeepArcticCalm => "calm",
        }
    }

    /// Case-insensitive parser from short or descriptive names.
    pub fn from_str_case_insensitive(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "default" | "legacy" | "0" => Some(Self::Default),
            "storm" | "classic" | "classic_storm" | "geomagnetic_storm" | "1" => {
                Some(Self::ClassicGeomagneticStorm)
            }
            "crimson" | "red" | "high_altitude_crimson" | "2" => Some(Self::HighAltitudeCrimson),
            "violet" | "dawn" | "polar_violet_dawn" | "blue" | "3" => Some(Self::PolarVioletDawn),
            "steve" | "ghostly_steve" | "sub_auroral_arc" | "purple" | "4" => {
                Some(Self::GhostlySteve)
            }
            "calm" | "deep_arctic_calm" | "arctic_calm" | "teal" | "5" => {
                Some(Self::DeepArcticCalm)
            }
            _ => None,
        }
    }

    /// Returns the 4 canonical spectral emission colors defining this preset.
    pub fn colors(&self) -> AuroraPaletteColors {
        match self {
            Self::ClassicGeomagneticStorm => AuroraPaletteColors {
                c0: glam::Vec3::new(0.08, 0.88, 0.38),
                c1: glam::Vec3::new(0.92, 0.15, 0.65),
                c2: glam::Vec3::new(0.45, 0.92, 0.70),
                c3: glam::Vec3::new(0.82, 0.08, 0.18),
            },
            Self::HighAltitudeCrimson => AuroraPaletteColors {
                c0: glam::Vec3::new(0.48, 0.05, 0.15),
                c1: glam::Vec3::new(0.85, 0.12, 0.22),
                c2: glam::Vec3::new(0.92, 0.40, 0.45),
                c3: glam::Vec3::new(0.65, 0.55, 0.78),
            },
            Self::PolarVioletDawn => AuroraPaletteColors {
                c0: glam::Vec3::new(0.10, 0.45, 0.95),
                c1: glam::Vec3::new(0.42, 0.18, 0.85),
                c2: glam::Vec3::new(0.80, 0.20, 0.75),
                c3: glam::Vec3::new(0.85, 0.50, 0.60),
            },
            Self::GhostlySteve => AuroraPaletteColors {
                c0: glam::Vec3::new(0.45, 0.65, 0.50),
                c1: glam::Vec3::new(0.55, 0.32, 0.58),
                c2: glam::Vec3::new(0.78, 0.45, 0.88),
                c3: glam::Vec3::new(0.28, 0.18, 0.45),
            },
            Self::DeepArcticCalm => AuroraPaletteColors {
                c0: glam::Vec3::new(0.05, 0.55, 0.48),
                c1: glam::Vec3::new(0.22, 0.75, 0.30),
                c2: glam::Vec3::new(0.50, 0.85, 0.68),
                c3: glam::Vec3::new(0.80, 0.65, 0.35),
            },
            Self::Default => AuroraPaletteColors {
                c0: glam::Vec3::new(0.08, 0.82, 0.42),
                c1: glam::Vec3::new(0.06, 0.74, 0.62),
                c2: glam::Vec3::new(0.08, 0.68, 0.82),
                c3: glam::Vec3::new(0.38, 0.18, 0.62),
            },
        }
    }
}

/// The 4 spectral emission colors defining a procedural aurora palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuroraPaletteColors {
    /// Base curtain body / lower boundary.
    pub c0: glam::Vec3,
    /// Energized fold / dynamic fringe.
    pub c1: glam::Vec3,
    /// Sharp filament / core highlight.
    pub c2: glam::Vec3,
    /// High-altitude upper flare.
    pub c3: glam::Vec3,
}

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
    /// Active procedural color preset. Default is `AuroraPaletteId::Default`.
    pub palette: AuroraPaletteId,
}

impl Default for AuroraParameters {
    fn default() -> Self {
        Self {
            intensity: 1.0,
            palette: AuroraPaletteId::Default,
        }
    }
}

impl AuroraParameters {
    /// Maximum allowed aurora intensity multiplier.
    pub const MAX_INTENSITY: f32 = 10.0;

    /// Uniform encoding stride for packed palette selector (std140 176-byte ABI preservation).
    pub const PALETTE_STRIDE: f32 = 16.0;

    /// Creates an aurora configuration with default intensity (`1.0`) and default palette.
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

    /// Sets the active procedural color palette preset.
    pub fn set_palette(&mut self, palette: AuroraPaletteId) {
        self.palette = palette;
    }

    /// Encodes intensity and palette selector into a single float for the 176-byte `SkyUniform`.
    ///
    /// Formula: `intensity + (palette_id as f32) * 16.0`.
    ///
    /// Invariant:
    /// - For `Default` (palette 0), returns `intensity` bitwise/identically.
    /// - Since `intensity <= 10.0 < 16.0`, the decoding in WGSL `u32(floor((encoded + 0.001) / 16.0))`
    ///   is 100% numerically stable and lossless across all hardware.
    #[inline]
    pub fn encoded_uniform_value(&self) -> f32 {
        self.intensity + (self.palette as u32 as f32) * Self::PALETTE_STRIDE
    }

    /// Decodes an encoded uniform float back into `(intensity, palette)`.
    pub fn decode_uniform_value(encoded: f32) -> (f32, AuroraPaletteId) {
        if !encoded.is_finite() || encoded < 0.0 {
            return (0.0, AuroraPaletteId::Default);
        }
        let raw_palette = ((encoded + 0.001) / Self::PALETTE_STRIDE).floor() as u32;
        let palette = AuroraPaletteId::from_u32(raw_palette);
        let intensity = (encoded - (palette as u32 as f32) * Self::PALETTE_STRIDE)
            .clamp(0.0, Self::MAX_INTENSITY);
        (intensity, palette)
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
    let effective_emission = AuroraParameters {
        intensity,
        ..Default::default()
    }
    .effective_emission(sun_elevation);

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
