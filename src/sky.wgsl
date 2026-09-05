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
    _pad0: f32,
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

    // Direction vector from camera eye towards far-plane sky
    out.view_dir = world_pos - sky.camera_pos;
    return out;
}

// Deterministic 3D hash for procedural star field
fn hash33(p: vec3<f32>) -> vec3<f32> {
    var p3 = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yxz + 33.33);
    return fract((p3.xxy + p3.yxx) * p3.zyx);
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

    return vec4<f32>(sky_color, 1.0);
}
