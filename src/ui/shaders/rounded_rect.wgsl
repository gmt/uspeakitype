struct ThemeColors {
    background: vec4<f32>,
    shadow: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> theme: ThemeColors;

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(vertex.position, 0.0, 1.0);
    out.uv = vertex.position * 0.5 + 0.5;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let center = vec2<f32>(0.5, 0.5);
    let size = vec2<f32>(1.0, 1.0);
    let corner_radius = 0.08;

    let shadow_offset = vec2<f32>(0.004, -0.004);
    let shadow_blur = 0.015;

    let shadow_center = center + shadow_offset;
    let half_size = size * 0.5;
    let shadow_dist = abs(in.uv - shadow_center) - half_size + corner_radius;
    let shadow_dist_to_edge = length(max(shadow_dist, vec2<f32>(0.0, 0.0))) - corner_radius;

    let shadow_alpha = 1.0 - clamp((shadow_dist_to_edge + shadow_blur) / shadow_blur, 0.0, 1.0);
    let shadow_color = vec4<f32>(theme.shadow.rgb, shadow_alpha * 0.4);

    let dist = abs(in.uv - center) - half_size + corner_radius;
    let dist_to_edge = length(max(dist, vec2<f32>(0.0, 0.0))) - corner_radius;

    let edge_width = 0.005;
    let main_alpha = 1.0 - clamp(dist_to_edge / edge_width + 0.5, 0.0, 1.0);

    let main_color = vec4<f32>(theme.background.rgb, main_alpha * 0.85);

    let result = mix(shadow_color, main_color, main_alpha);

    return result;
}
