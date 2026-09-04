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

    // 2. Procedural Sun Disc & Corona Glow
    let sun_dir = normalize(sky.sun_direction);
    let cos_sun = dot(dir, sun_dir);
    if (cos_sun > 0.0) {
        let sun_core = smoothstep(0.9985, 0.9998, cos_sun);
        let sun_glow = pow(cos_sun, 32.0) * 0.45;
        let sun_wide = pow(cos_sun, 4.0) * 0.15 * sky.twilight_factor;
        let sun_contrib = sky.sun_color * (sun_core * 2.0 + sun_glow + sun_wide);
        sky_color += sun_contrib;
    }

    // 3. Procedural Moon Disc & Continuous Phase Shading
    let moon_dir = normalize(sky.moon_direction);
    let cos_moon = dot(dir, moon_dir);
    if (cos_moon > 0.9980) {
        let moon_core = smoothstep(0.9986, 0.9996, cos_moon);

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

            let moon_diffuse = max(0.0, dot(normal_moon, light_dir_moon));
            let earthshine = 0.05; // Faint visibility of unlit moon face
            let moon_intensity = moon_core * (moon_diffuse * 0.95 + earthshine);

            let moon_color = vec3<f32>(0.82, 0.88, 0.98) * moon_intensity;
            sky_color += moon_color;
        }
    }

    // 4. Procedural Deterministic Stars
    if (sky.star_visibility > 0.005) {
        let star_grid = dir * 160.0;
        let cell = floor(star_grid);
        let frac_coord = fract(star_grid);
        let rnd = hash33(cell);

        // Approximately 1.8% of cells contain a visible star point
        if (rnd.x > 0.982) {
            let star_center = rnd * 0.7 + 0.15;
            let dist = length(frac_coord - star_center);
            let star_point = smoothstep(0.07, 0.015, dist);

            // Subtle twinkle oscillation without moving star position
            let twinkle = 0.8 + 0.2 * sin(sky.bounded_time * 3.5 + rnd.y * 62.83);
            let star_tint = mix(vec3<f32>(0.85, 0.92, 1.0), vec3<f32>(1.0, 0.92, 0.82), rnd.z);

            let stars = star_tint * (star_point * twinkle * sky.star_visibility * 1.6);
            sky_color += stars;
        }
    }

    return vec4<f32>(sky_color, 1.0);
}
