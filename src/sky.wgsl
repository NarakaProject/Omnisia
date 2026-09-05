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

    // 5. Procedural Aurora Curtains (Phase 10.6.1: Atmospheric Curtain Morphology & Depth)
    // Mandatory Amendment A: Ascending smoothstep edges (1.0 - smoothstep(-0.18, -0.06, sun_elevation))
    let aurora_visibility = 1.0 - smoothstep(-0.18, -0.06, sky.sun_elevation);
    let effective_aurora_strength = sky.aurora_intensity * aurora_visibility;

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

            // Multi-Rate Bounded Temporal Frequencies (Sections 14 & 15):
            // All phases derive deterministically from bounded_time (in [0, 60)) with integer harmonics
            // of 2*PI/60 to guarantee 100% pop-free 60-second wrapping, while operating at different
            // spatial scales and speeds to avoid synchronized carousel sliding:
            let tau = 6.2831853;
            let omega0 = tau / 60.0;
            let time_macro     = sky.bounded_time * omega0;         // k=1 (60s fundamental: slow curtain drift)
            let time_curtain   = sky.bounded_time * (omega0 * 2.0); // k=2 (30s: fold undulation)
            let time_filaments = sky.bounded_time * (omega0 * 3.0); // k=3 (20s: filament brightening/fading)
            let time_shimmer   = sky.bounded_time * (omega0 * 4.0); // k=4 (15s: fine streamer shimmer)

            // Vertical envelope: soft horizon fade and graceful zenith thinning
            let horizon_fade = smoothstep(0.04, 0.16, dir.y);
            let zenith_fade = 1.0 - smoothstep(0.60, 0.88, dir.y);
            let vertical_envelope = horizon_fade * zenith_fade;

            // ================================================================
            // LAYER 1: FAR CURTAIN (Distant, soft, luminous background sheet)
            // ================================================================
            let uv_far = pos_far * 0.00030;
            let far_noise = smooth_noise2d(uv_far + vec2<f32>(time_macro * 0.35, -time_macro * 0.25));
            let far_spine = sin(uv_far.x * 1.2 + time_macro) * 0.85 + cos(uv_far.x * 0.55 - time_macro * 0.4) * 0.45;
            let d_far = abs(uv_far.y + 1.8 - far_spine - far_noise * 0.5);
            let far_sheet = smoothstep(0.60, 0.05, d_far);
            let far_curtain = far_sheet * (0.50 + 0.50 * far_noise);

            // ================================================================
            // LAYER 2: MAIN CURTAIN (Full Morphology Pipeline)
            // ================================================================
            let uv_main = pos_main * 0.00055;

            // 1. Spatial Field & 2D Vector Domain Warping:
            // Introduces large-scale bends, compression, and expansion in 2D space:
            let warp_seed = uv_main + vec2<f32>(time_macro * 0.45, time_curtain * 0.25);
            let macro_noise = smooth_noise2d(warp_seed);
            let warp_vec = vec2<f32>(
                sin(uv_main.y * 1.8 + time_curtain) * 0.45 + (macro_noise - 0.5) * 0.85,
                cos(uv_main.x * 1.3 - time_macro) * 0.40 + (macro_noise - 0.5) * 0.65
            );
            let q_main = uv_main + warp_vec;

            // 2. Curtain Sheet Envelope on Warped Manifold:
            let fold_wave = sin(q_main.x * 1.6 + time_curtain) * 0.65
                + cos(q_main.x * 0.78 - time_macro * 0.6) * 0.50;
            let d_main = abs(q_main.y + 1.3 - fold_wave);
            let main_sheet = smoothstep(0.40, 0.03, d_main);

            // 3. Meso-scale Thickness & Organic Gap Modulation:
            let meso_p = vec2<f32>(q_main.x * 2.1 + time_curtain * 0.5, q_main.y * 1.4 - time_macro * 0.8);
            let meso_noise = smooth_noise2d(meso_p);
            let thickness_mod = 0.55 + 0.45 * meso_noise;

            // 4. Organically Ragged Lower Edge (Section 7):
            let ragged_bottom = 0.07 + 0.05 * sin(q_main.x * 2.2 + time_macro) + 0.04 * meso_noise;
            let ragged_lower_fade = smoothstep(ragged_bottom, ragged_bottom + 0.12, dir.y);

            // 5. Clustered & Broken Vertical Filaments:
            // The carrier uses incommensurate frequencies (phi ~ 1.618, e ~ 2.718, sqrt(2)+e ~ 4.132)
            // evaluated on the warped manifold Q, filtered through a 2D cluster mask (gaps)
            // and vertical break noise (streamers/nodes along dir.y):
            let fil_coord = q_main.x * 5.8 + meso_noise * 2.5 + warp_vec.x * 1.6;
            let ray_a = sin(fil_coord * 1.618 + time_filaments);
            let ray_b = sin(fil_coord * 2.718 - time_filaments * 0.75 + 1.3);
            let ray_c = cos(fil_coord * 4.132 + time_shimmer + meso_noise * 1.7);
            let carrier = ray_a * 0.45 + ray_b * 0.35 + ray_c * 0.20;
            let ray_sharp = pow(max(0.0, carrier * 0.5 + 0.5), 3.2);

            // 2D Spatial cluster mask: creates dense ray clusters and sparse calm regions
            let cluster_mask = smoothstep(0.25, 0.75, meso_noise);

            // Vertical streamer breaks along dir.y: breaks rays vertically into discrete nodes
            let break_noise = smooth_noise2d(vec2<f32>(fil_coord * 0.35, dir.y * 5.5 - time_filaments * 0.45));
            let break_mask = smoothstep(0.20, 0.80, break_noise);

            let main_filaments = (0.25 + 0.75 * ray_sharp * cluster_mask) * (0.40 + 0.60 * break_mask);
            let main_curtain = main_sheet * thickness_mod * ragged_lower_fade * main_filaments;

            // ================================================================
            // LAYER 3: FINE FILAMENT LAYER (Sparse, high-energy foreground rays)
            // ================================================================
            let uv_fine = pos_fine * 0.00095;
            let fine_warp = smooth_noise2d(uv_fine * 1.3 + vec2<f32>(-time_filaments * 0.35, time_shimmer * 0.25));
            let fine_coord = uv_fine.x * 8.8 + fine_warp * 3.2 + time_filaments;
            let fine_ray1 = sin(fine_coord * 1.732 - time_shimmer);
            let fine_ray2 = cos(fine_coord * 3.141 + time_filaments * 1.1);
            let fine_sharp = pow(max(0.0, fine_ray1 * 0.55 + fine_ray2 * 0.45), 4.0);

            let fine_break = smooth_noise2d(vec2<f32>(fine_coord * 0.45, dir.y * 6.5 + time_shimmer * 0.5));
            let fine_filaments = fine_sharp * smoothstep(0.40, 0.85, fine_break) * main_sheet;

            // ================================================================
            // ENERGY-DRIVEN COLOR MODEL (Sections 12 & 26)
            // ================================================================
            // Local energy derives from physical structure and filament convergence, not Y
            let local_energy = clamp(main_curtain * 0.85 + fine_filaments * 1.10 + far_curtain * 0.25, 0.0, 1.5);

            // Palette definitions:
            let col_deep_emerald = vec3<f32>(0.08, 0.82, 0.42);  // Dominant curtain body
            let col_turquoise    = vec3<f32>(0.06, 0.74, 0.62);  // Energized folds
            let col_bright_cyan  = vec3<f32>(0.08, 0.68, 0.82);  // High-energy filament peaks
            let col_soft_violet  = vec3<f32>(0.38, 0.18, 0.62);  // Restrained upper tip flare

            // Base transition: emerald -> turquoise based on local energy
            let energy_t = clamp(local_energy * 0.8, 0.0, 1.0);
            let base_aurora = mix(col_deep_emerald, col_turquoise, energy_t);

            // Bright cyan accents on sharp filament peaks
            let cyan_t = clamp(fine_filaments * 1.6 + (ray_sharp - 0.5) * 0.6, 0.0, 1.0);
            let with_cyan = mix(base_aurora, col_bright_cyan, cyan_t * 0.65);

            // Restrained violet accent: strictly confined to high-energy upper filament tips
            let upper_reaches = smoothstep(0.32, 0.72, dir.y);
            let violet_energy = clamp((local_energy - 0.45) * 1.8, 0.0, 1.0);
            let violet_t = violet_energy * upper_reaches * 0.32; // strictly restrained accent (max 32%)
            let aurora_color = mix(with_cyan, col_soft_violet, violet_t);

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
