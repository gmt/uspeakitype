struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct IconUniforms {
    offset: vec2<f32>,
    scale: vec2<f32>,
}

@group(0) @binding(0)
var icon_texture: texture_2d<f32>;

@group(0) @binding(1)
var icon_sampler: sampler;

@group(1) @binding(0)
var<uniform> uniforms: IconUniforms;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(
        uniforms.offset.x + in.position.x * uniforms.scale.x,
        uniforms.offset.y - in.position.y * uniforms.scale.y,
        0.0,
        1.0
    );
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(icon_texture, icon_sampler, in.uv);
}
