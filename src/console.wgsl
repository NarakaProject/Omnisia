// Omnisia Developer Console 2D Overlay Shader

struct ScreenUniform {
    screen_size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> screen: ScreenUniform;
@group(0) @binding(1) var font_texture: texture_2d<f32>;
@group(0) @binding(2) var font_sampler: sampler;

struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Map screen pixel coordinates [0, width] x [0, height] to NDC [-1, 1] x [1, -1]
    let ndc_x = (in.pos.x / screen.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (in.pos.y / screen.screen_size.y) * 2.0;

    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Negative UV.x indicates untextured solid color (background panel, borders, cursor)
    if (in.uv.x < 0.0) {
        return in.color;
    }

    // Sample 1-channel glyph alpha from bitmap font atlas
    let glyph_alpha = textureSample(font_texture, font_sampler, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * glyph_alpha);
}
