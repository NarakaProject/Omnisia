struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad0: f32,
};

struct LightUniform {
    sun_direction: vec3<f32>,
    _pad1: f32,
    sun_color: vec3<f32>,
    _pad2: f32,
    ambient_color: vec3<f32>,
    _pad3: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(0) @binding(1)
var<uniform> light: LightUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) ao: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) ao: f32,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.world_normal = model.normal;
    out.color = model.color;
    out.ao = model.ao;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let L = normalize(-light.sun_direction);

    // 1. Shading Half-Lambert: diffuse = (N · L * 0.5 + 0.5)^2 for N · L > 0.
    // Surfaces facing away (N · L <= 0) receive zero direct celestial light (Mandates 3 & 4).
    let n_dot_l = dot(N, L);
    var diffuse_factor: f32 = 0.0;
    if (n_dot_l > 0.0) {
        let half_lambert = n_dot_l * 0.5 + 0.5;
        diffuse_factor = half_lambert * half_lambert;
    }

    // 2. Direct Celestial Light & Ambient Fill evaluated independently
    let direct_light = light.sun_color * diffuse_factor;
    let ao_modulated = in.ao * 0.7 + 0.3; // Prevent corner occlusion from being pitch black
    let ambient_light = light.ambient_color * ao_modulated;

    // 3. Composition of Total Lighting & Base Color
    let total_lighting = direct_light + ambient_light;
    let final_rgb = in.color * total_lighting;

    return vec4<f32>(final_rgb, 1.0);
}
