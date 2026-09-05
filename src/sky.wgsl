struct SkyUniform {
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    bounded_time: f32,
    sun_direction: vec3<f32>,
    sun_elevation: f32,
    moon_direction: vec3<f32>,
    moon_phase: f32,
    sun_color: vec3<f32>,
    twilight_factor: f32,
    ambient_color: vec3<f32>,
    star_visibility: f32,
    horizon_color: vec3<f32>,
    day_factor: f32,
    zenith_color: vec3<f32>,
    aurora_intensity: f32,
};

@group(0) @binding(0)
var<uniform> sky: SkyUniform;

struct SkyVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) view_dir: vec3<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) vertex_index: u32) -> SkyVertexOutput {
    var out: SkyVertexOutput;

    // Fullscreen triangle generation without vertex buffer allocations
    // Vertices: (-1, -1), (3, -1), (-1, 3)
    let x = f32((vertex_index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vertex_index & 2u) * 2.0 - 1.0;

    // Depth = 1.0 at clip space far plane for depth-buffer testing
    out.clip_position = vec4<f32>(x, y, 1.0, 1.0);

    // Unproject clip space coordinate to world space position
    let clip_pos = vec4<f32>(x, y, 1.0, 1.0);
    let world_h = sky.inv_view_proj * clip_pos;
    let world_pos = world_h.xyz / world_h.w;

    // Direction vector from origin towards far-plane celestial sphere
    out.view_dir = world_pos;
    return out;
}

// Deterministic 3D hash for procedural star field
fn hash33(p: vec3<f32>) -> vec3<f32> {
    var p3 = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yxz + 33.33);
    return fract((p3.xxy + p3.yxx) * p3.zyx);
}

// Deterministic 2D hash for procedural aurora
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Smooth 2D procedural value noise with cubic Hermite interpolation
fn smooth_noise2d(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let a00 = hash21(i + vec2<f32>(0.0, 0.0));
    let a10 = hash21(i + vec2<f32>(1.0, 0.0));
    let a01 = hash21(i + vec2<f32>(0.0, 1.0));
    let a11 = hash21(i + vec2<f32>(1.0, 1.0));

    return mix(mix(a00, a10, u.x), mix(a01, a11, u.x), u.y);
}

// Procedural aurora palette definition (Phase 10.6.1R)
struct AuroraPalette {
    c0: vec3<f32>, // Base curtain body / lower boundary
    c1: vec3<f32>, // Energized fold / dynamic fringe
    c2: vec3<f32>, // Sharp filament / core highlight
    c3: vec3<f32>, // High-altitude upper flare
};

// Resolves solely the 4 required color constants for the active preset via uniform branch
fn get_aurora_palette(id: u32) -> AuroraPalette {
    var p: AuroraPalette;
    switch (id) {
        case 1u: { // CLASSIC_GEOMAGNETIC_STORM
            p.c0 = vec3<f32>(0.08, 0.88, 0.38); // Bright Emerald (primary oxygen emission)
            p.c1 = vec3<f32>(0.92, 0.15, 0.65); // Magenta / Hot Pink (lower nitrogen fringe)
            p.c2 = vec3<f32>(0.45, 0.92, 0.70); // Pale Mint Green (filament peaks)
            p.c3 = vec3<f32>(0.82, 0.08, 0.18); // Deep Crimson (high-altitude oxygen glow)
        }
        case 2u: { // HIGH_ALTITUDE_CRIMSON
            p.c0 = vec3<f32>(0.48, 0.05, 0.15); // Deep Wine / Burgundy
            p.c1 = vec3<f32>(0.85, 0.12, 0.22); // Rich Crimson
            p.c2 = vec3<f32>(0.92, 0.40, 0.45); // Coral Pink
            p.c3 = vec3<f32>(0.65, 0.55, 0.78); // Faint Lavender
        }
        case 3u: { // POLAR_VIOLET_DAWN
            p.c0 = vec3<f32>(0.10, 0.45, 0.95); // Electric Blue
            p.c1 = vec3<f32>(0.42, 0.18, 0.85); // Cobalt Violet
            p.c2 = vec3<f32>(0.80, 0.20, 0.75); // Neon Orchid / Magenta
            p.c3 = vec3<f32>(0.85, 0.50, 0.60); // Soft Rose / Dusty Pink
        }
        case 4u: { // GHOSTLY_STEVE (Faint Sage -> Mauve -> Bright Lilac -> Smoky Indigo)
            p.c0 = vec3<f32>(0.45, 0.65, 0.50); // Faint Sage Green
            p.c1 = vec3<f32>(0.55, 0.32, 0.58); // Mauve / Dusty Purple
            p.c2 = vec3<f32>(0.78, 0.45, 0.88); // Bright Lilac
            p.c3 = vec3<f32>(0.28, 0.18, 0.45); // Smoky Indigo
        }
        case 5u: { // DEEP_ARCTIC_CALM
            p.c0 = vec3<f32>(0.05, 0.55, 0.48); // Forest Teal
            p.c1 = vec3<f32>(0.22, 0.75, 0.30); // Spring Apple Green
            p.c2 = vec3<f32>(0.50, 0.85, 0.68); // Pale Seafoam
            p.c3 = vec3<f32>(0.80, 0.65, 0.35); // Faint Warm Amber
        }
        default: { // DEFAULT / LEGACY_CYAN_VIOLET
            p.c0 = vec3<f32>(0.08, 0.82, 0.42); // Deep Emerald
            p.c1 = vec3<f32>(0.06, 0.74, 0.62); // Turquoise
            p.c2 = vec3<f32>(0.08, 0.68, 0.82); // Bright Cyan
            p.c3 = vec3<f32>(0.38, 0.18, 0.62); // Soft Violet
        }
    }
    return p;
}

@fragment
fn fs_sky(in: SkyVertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.view_dir);

    // 1. Atmospheric Gradient
    var sky_color: vec3<f32>;
    if (dir.y >= 0.0) {
        let h = pow(dir.y, 0.55);
        sky_color = mix(sky.horizon_color, sky.zenith_color, h);
    } else {
        let ground_tint = sky.ambient_color * 0.35;
        let nadir_h = pow(-dir.y, 0.5);
        sky_color = mix(sky.horizon_color, ground_tint, nadir_h);
    }

    // 2. Procedural Sun Disc, Corona Glow & Atmospheric Transition Extinction
    let sun_dir = normalize(sky.sun_direction);
    let cos_sun = dot(dir, sun_dir);

    // Solar Atmospheric Extinction Invariants (Additional Objective):
    // - Direct sun disc fades smoothly as lower limb crosses horizon, strictly 0.0 when sun_elevation <= -0.02
    // - Geometric horizon occlusion: direct sun disc is never visible below the horizon plane (dir.y > -0.01)
    // - Solar halo / twilight corona lingers across civil twilight (-0.12 <= elevation <= 0.02) and fades to 0.0 before deep night
    let sun_disc_extinction = smoothstep(-0.02, 0.05, sky.sun_elevation);
    let sun_halo_extinction = smoothstep(-0.12, 0.02, sky.sun_elevation);

    if (cos_sun > 0.0 && sun_halo_extinction > 0.0) {
        let sun_core = smoothstep(0.9985, 0.9998, cos_sun);
        let horizon_clip = select(0.0, 1.0, dir.y > -0.01);
        let direct_sun = sun_core * 2.0 * sun_disc_extinction * horizon_clip;

        let sun_glow = pow(cos_sun, 32.0) * 0.45 * sun_halo_extinction;
        let sun_wide = pow(cos_sun, 4.0) * 0.15 * sky.twilight_factor * sun_halo_extinction;
        let sun_contrib = sky.sun_color * (direct_sun + sun_glow + sun_wide);
        sky_color += sun_contrib;
    }

    // 3. Procedural Moon Disc, Phase Shading & Refined Halo Glow (Section 14-19)
    let moon_dir = normalize(sky.moon_direction);
    let cos_moon = dot(dir, moon_dir);

    // Restrained, soft atmospheric moon halo:
    // Spread wide (cos > 0.95), soft falloff, dimmer than disc (Section 17)
    // Visual hierarchy: MOON CORE (2.85) >> MOON HALO (0.035) > STARS (0.2-0.3) > NIGHT SKY (0.02)
    if (cos_moon > 0.95 && sky.star_visibility > 0.05) {
        let halo_dist = max(0.0, (cos_moon - 0.95) / 0.05);
        let moon_halo = pow(halo_dist, 5.0) * 0.035 * (1.0 - sky.day_factor);
        sky_color += vec3<f32>(0.25, 0.35, 0.55) * moon_halo;
    }

    if (cos_moon > 0.9980) {
        let moon_core = smoothstep(0.9979, 0.9985, cos_moon);

        // Orthonormal tangent frame on moon disc
        var up_hint = vec3<f32>(0.0, 1.0, 0.0);
        if (abs(moon_dir.y) > 0.95) {
            up_hint = vec3<f32>(1.0, 0.0, 0.0);
        }
        let moon_tangent = normalize(cross(up_hint, moon_dir));
        let moon_bitangent = cross(moon_dir, moon_tangent);

        // Project ray offset onto 2D moon disc coordinates
        let offset = dir - moon_dir * cos_moon;
        let u = dot(offset, moon_tangent) / 0.038;
        let v = dot(offset, moon_bitangent) / 0.038;
        let r2 = u * u + v * v;

        if (r2 <= 1.0) {
            let w = sqrt(max(0.0, 1.0 - r2));
            let normal_moon = u * moon_tangent + v * moon_bitangent + w * moon_dir;

            // Continuous phase illumination angle (TAU * moon_phase)
            let phase_angle = sky.moon_phase * 6.2831853;
            let light_dir_moon = sin(phase_angle) * moon_tangent + cos(phase_angle) * moon_dir;

            let n_dot_l_moon = dot(normal_moon, light_dir_moon);
            let moon_diffuse = max(0.0, n_dot_l_moon);
            let earthshine = 0.035; // Faint visibility of unlit moon face
            // Luminous moon crescent (clearly brighter than halo, but << daytime sun)
            let moon_crescent = pow(moon_diffuse, 0.85) * 2.85;
            let moon_intensity = moon_core * (moon_crescent + earthshine);

            let moon_color = vec3<f32>(0.92, 0.95, 1.0) * moon_intensity;
            sky_color += moon_color;
        }
    }

    // 4. Procedural Deterministic Stars (Mandates 10, 11, 12)
    if (sky.star_visibility > 0.005) {
        let star_grid = dir * 140.0;
        let cell = floor(star_grid);
        let frac_coord = fract(star_grid);
        let rnd = hash33(cell);

        // Filter: cell selection threshold and elevation horizon cutoff
        if (rnd.x > 0.975 && dir.y > -0.02) {
            let star_center = rnd * 0.6 + 0.2;
            let dist = length(frac_coord - star_center);
            let star_radius = 0.20;
            let star_point = smoothstep(star_radius, 0.03, dist);

            // Horizon atmospheric extinction fade
            let horizon_fade = smoothstep(-0.02, 0.06, dir.y);

            // Subtle temporal twinkle oscillation preserving spatial position
            let twinkle = 0.75 + 0.25 * sin(sky.bounded_time * 3.5 + rnd.y * 62.83);

            // Star magnitude / brightness variation (power distribution for natural diversity)
            let magnitude = pow(rnd.y, 3.0) * 2.5 + 0.8;

            // Spectral color temperature tint
            let star_tint = mix(vec3<f32>(0.82, 0.90, 1.0), vec3<f32>(1.0, 0.88, 0.75), rnd.z);

            // Moon proximity attenuation: suppress stars immediately around the bright moon disc
            let moon_dist = max(0.0, 1.0 - cos_moon);
            let moon_occlusion = smoothstep(0.001, 0.008, moon_dist);

            let star_radiance = star_point * magnitude * horizon_fade * twinkle * sky.star_visibility * moon_occlusion;
            let stars = star_tint * star_radiance;
            sky_color += stars;
        }
    }

    // 5. Procedural Aurora Curtains (Phase 10.6.1R: Performance Recovery, Stabilization & Color Presets)
    // Mandatory Amendment A: Ascending smoothstep edges (1.0 - smoothstep(-0.18, -0.06, sun_elevation))
    let aurora_visibility = 1.0 - smoothstep(-0.18, -0.06, sky.sun_elevation);

    // Decode packed palette selector and true intensity from sky.aurora_intensity (176-byte ABI preserved):
    let raw_palette = floor((sky.aurora_intensity + 0.001) / 16.0);
    let palette_id = min(u32(raw_palette), 5u);
    let real_intensity = sky.aurora_intensity - f32(palette_id) * 16.0;
    let effective_aurora_strength = real_intensity * aurora_visibility;

    if (effective_aurora_strength > 0.001 && dir.y > 0.03) {
        // Facing direction anchor along -Z world axis (Amendment C & Section 16).
        // Uses strictly ordered smoothstep edges: 1.0 - smoothstep(-0.55, 0.25, dir.z).
        let anchor_alignment = 1.0 - smoothstep(-0.55, 0.25, dir.z);

        if (anchor_alignment > 0.001) {
            // Altitude stability guard: clamps camera Y to [-100.0, 4000.0] to prevent layer collapse,
            // coordinate singularities, and negative effective heights at extreme altitudes (e.g. Y=5000m).
            let clamped_cam_y = clamp(sky.camera_pos.y, -100.0, 4000.0);
            let safe_dy = max(dir.y, 0.04);

            // Three distinct atmospheric layer heights:
            // h_far > h_main > h_fine holds strictly across ALL altitudes in [-100, 5000]m
            let h_far  = max(400.0, 2400.0 - clamped_cam_y * 0.15);
            let h_main = max(400.0, 1500.0 - clamped_cam_y * 0.20);
            let h_fine = max(400.0, 1050.0 - clamped_cam_y * 0.25);

            let t_far  = h_far / safe_dy;
            let t_main = h_main / safe_dy;
            let t_fine = h_fine / safe_dy;

            let pos_far  = vec2<f32>(sky.camera_pos.x + t_far * dir.x,  sky.camera_pos.z + t_far * dir.z);
            let pos_main = vec2<f32>(sky.camera_pos.x + t_main * dir.x, sky.camera_pos.z + t_main * dir.z);
            let pos_fine = vec2<f32>(sky.camera_pos.x + t_fine * dir.x, sky.camera_pos.z + t_fine * dir.z);

            // Bounded Temporal Frequencies with Strict Closed-Loop Continuity (REG-2 Resolution):
            // All phase terms use strict integer harmonics (k = 1, 2, 3, 4) of omega0 = 2*PI/60.
            // Noise coordinate offsets use closed-loop circular phase (cos/sin of theta),
            // ensuring that at t = 0.0 and t = 60.0 the noise sampling coordinates are identical.
            let tau = 6.2831853;
            let omega0 = tau / 60.0;
            let theta = sky.bounded_time * omega0; // in [0, 2*PI)
            let cos_t1 = cos(theta);
            let sin_t1 = sin(theta);
            let cos_t2 = cos(theta * 2.0);
            let sin_t2 = sin(theta * 2.0);

            let time_macro     = theta;       // k=1 (60s fundamental: slow curtain drift)
            let time_curtain   = theta * 2.0; // k=2 (30s: fold undulation)
            let time_filaments = theta * 3.0; // k=3 (20s: filament brightening/fading)
            let time_shimmer   = theta * 4.0; // k=4 (15s: fine streamer shimmer)

            // Vertical envelope: soft horizon fade and graceful zenith thinning
            let horizon_fade = smoothstep(0.04, 0.16, dir.y);
            let zenith_fade = 1.0 - smoothstep(0.60, 0.88, dir.y);
            let vertical_envelope = horizon_fade * zenith_fade;

            // ================================================================
            // LAYER 2: MAIN CURTAIN (Full Morphology Pipeline, 2 Noise Calls)
            // ================================================================
            let uv_main = pos_main * 0.00055;

            // 1. Spatial Field & 2D Vector Domain Warping:
            // Closed-loop elliptical orbit for noise seed ensures zero jump across 60s wrap
            let macro_drift = vec2<f32>(cos_t1 * 0.45, sin_t1 * 0.35);
            let macro_noise = smooth_noise2d(uv_main + macro_drift); // NOISE CALL 1
            let warp_vec = vec2<f32>(
                sin(uv_main.y * 1.8 + time_curtain) * 0.45 + (macro_noise - 0.5) * 0.85,
                cos(uv_main.x * 1.3 - time_macro) * 0.40 + (macro_noise - 0.5) * 0.65
            );
            let q_main = uv_main + warp_vec;

            // 2. Curtain Sheet Envelope on Warped Manifold:
            let fold_wave = sin(q_main.x * 1.6 + time_curtain) * 0.65
                + cos(q_main.x * 0.78 - time_macro) * 0.50;
            let d_main = abs(q_main.y + 1.3 - fold_wave);
            let main_sheet = 1.0 - smoothstep(0.03, 0.40, d_main);

            // 3. Meso-scale Thickness & Organic Gap Modulation:
            let meso_drift = vec2<f32>(cos_t2 * 0.35, sin_t1 * 0.45);
            let meso_p = vec2<f32>(q_main.x * 2.1, q_main.y * 1.4) + meso_drift;
            let meso_noise = smooth_noise2d(meso_p); // NOISE CALL 2
            let thickness_mod = 0.55 + 0.45 * meso_noise;

            // 4. Organically Ragged Lower Edge:
            let ragged_bottom = 0.07 + 0.05 * sin(q_main.x * 2.2 + time_macro) + 0.04 * meso_noise;
            let ragged_lower_fade = smoothstep(ragged_bottom, ragged_bottom + 0.12, dir.y);

            // 5. Clustered & Broken Vertical Filaments:
            let fil_coord = q_main.x * 5.8 + meso_noise * 2.5 + warp_vec.x * 1.6;
            let ray_a = sin(fil_coord * 1.618 + time_filaments);
            let ray_b = sin(fil_coord * 2.718 - time_curtain + 1.3);
            let ray_c = cos(fil_coord * 4.132 + time_shimmer);
            let carrier = max(0.0, (ray_a * 0.45 + ray_b * 0.35 + ray_c * 0.20) * 0.5 + 0.5);
            let ray_sharp = carrier * carrier * carrier;

            // 2D Spatial cluster mask:
            let cluster_mask = smoothstep(0.25, 0.75, meso_noise);

            // Fast analytic vertical streamer break along dir.y (replaces 2D noise with zero hash cost):
            let streamer_wave = sin(fil_coord * 0.85 + dir.y * 14.0 + time_filaments)
                * cos(fil_coord * 0.42 - dir.y * 8.0 + time_macro);
            let break_mask = smoothstep(-0.20, 0.70, streamer_wave * 0.55 + meso_noise * 0.45);

            let main_filaments = (0.25 + 0.75 * ray_sharp * cluster_mask) * (0.40 + 0.60 * break_mask);
            let main_curtain = main_sheet * thickness_mod * ragged_lower_fade * main_filaments;

            // ================================================================
            // LAYER 1: FAR CURTAIN (Distant, soft, luminous background sheet)
            // ================================================================
            // Derived directly from macro manifold and pos_far with differential parallax,
            // eliminating dedicated noise evaluation while preserving layered depth:
            let uv_far = pos_far * 0.00030;
            let far_spine = sin(uv_far.x * 1.2 + time_macro) * 0.85 + cos(uv_far.x * 0.55 - time_macro) * 0.45;
            let d_far = abs(uv_far.y + 1.8 - far_spine - (macro_noise - 0.5) * 0.5);
            let far_sheet = 1.0 - smoothstep(0.05, 0.60, d_far);
            let far_curtain = far_sheet * (0.50 + 0.50 * macro_noise);

            // ================================================================
            // LAYER 3: FINE FILAMENT LAYER (Sparse, high-energy foreground rays)
            // ================================================================
            // Derived from pos_fine and shared macro/meso state with fast power approximation:
            let fine_coord = pos_fine.x * 0.0055 + (macro_noise - 0.5) * 2.8 + time_filaments;
            let fine_ray1 = sin(fine_coord * 1.732 - time_shimmer);
            let fine_ray2 = cos(fine_coord * 3.141 + time_filaments);
            let fine_carrier = max(0.0, fine_ray1 * 0.55 + fine_ray2 * 0.45);
            let fine_c2 = fine_carrier * fine_carrier;
            let fine_sharp = fine_c2 * fine_c2; // fast x^4 without pow()

            let fine_break = smoothstep(0.35, 0.80, meso_noise * 0.60 + (streamer_wave * 0.5 + 0.5) * 0.40);
            let fine_filaments = fine_sharp * fine_break * main_sheet;

            // ================================================================
            // PALETTE RESOLUTION & ENERGY-DRIVEN PROCEDURAL COLOR MODEL
            // ================================================================
            // Resolve 4 palette colors for active preset with uniform branch (zero warp divergence):
            let pal = get_aurora_palette(palette_id);

            // Local auroral energy derived from physical curtain convergence and filament sharpness:
            let local_energy = clamp(main_curtain * 0.85 + fine_filaments * 1.10 + far_curtain * 0.25, 0.0, 1.5);
            let fold_t = clamp(local_energy * 0.80, 0.0, 1.0);
            let lower_fringe = (1.0 - smoothstep(0.06, 0.22, dir.y)) * clamp(local_energy * 1.4, 0.0, 1.0);

            // 1. Base transition between primary body (c0) and energized folds / lower fringe (c1):
            let fold_mix = clamp(fold_t * 1.25, 0.0, 1.0);
            let base_color = mix(pal.c0, pal.c1, max(fold_mix, lower_fringe));

            // 2. Sharp filament / core peak accent (c2):
            let fil_intensity = clamp(fine_filaments * 2.2 + ray_sharp * cluster_mask * 0.85, 0.0, 1.0);
            let filament_t = smoothstep(0.15, 0.85, fil_intensity);
            let with_filaments = mix(base_color, pal.c2, filament_t * 0.85);

            // 3. High-altitude upper reaches flare (c3):
            let upper_reaches = smoothstep(0.28, 0.68, dir.y);
            let upper_flare = upper_reaches * clamp(local_energy * 0.90 + 0.15, 0.0, 1.0);
            let aurora_color = mix(with_filaments, pal.c3, upper_flare * 0.75);

            // ================================================================
            // COMPOSITION & RADIANCE HIERARCHY (Section 13)
            // ================================================================
            let total_intensity = (far_curtain * 0.28 + main_curtain * 0.72 + fine_filaments * 0.35)
                * vertical_envelope
                * anchor_alignment
                * effective_aurora_strength;

            // Bounded radiance: peak aurora radiance <= 0.70 (subordinate to Moon core 2.85)
            let aurora_radiance = aurora_color * min(total_intensity, 0.70);

            // Translucent emissive composition: stars already in sky_color remain visible through curtains
            sky_color += aurora_radiance;
        }
    }

    return vec4<f32>(sky_color, 1.0);
}
