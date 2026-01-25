struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) instance_position: vec2<f32>,
    @location(2) instance_size: vec2<f32>,
    @location(3) instance_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_position = in.position * in.instance_size + in.instance_position;
    out.clip_position = vec4<f32>(world_position, 0.0, 1.0);
    out.color = in.instance_color;
    out.uv = world_position * 0.5 + 0.5;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // SDF clipping to rounded rectangle corners
    let corner_radius = 0.08;
    let center = vec2<f32>(0.5, 0.5);
    let half_size = vec2<f32>(0.5, 0.5);
    
    let dist = abs(in.uv - center) - half_size + corner_radius;
    let dist_to_edge = length(max(dist, vec2<f32>(0.0, 0.0))) - corner_radius;
    
    if dist_to_edge > 0.0 {
        discard;
    }
    
    return in.color;
}
