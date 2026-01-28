struct PanelUniforms {
    rect: vec4<f32>,   // [ndc_left, ndc_top, ndc_width, ndc_height]
    color: vec4<f32>,  // [r, g, b, a] where a=1.0 always for opacity independence
}

@group(0) @binding(0)
var<uniform> panel: PanelUniforms;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
    // Generate quad from vertex index (TriangleStrip: 0-1-2-3)
    // idx=0: top-left, idx=1: top-right, idx=2: bottom-left, idx=3: bottom-right
    let x_off = f32(idx & 1u);           // 0, 1, 0, 1
    let y_off = f32((idx >> 1u) & 1u);   // 0, 0, 1, 1
    
    let x = panel.rect.x + x_off * panel.rect.z;           // left + offset * width
    let y = panel.rect.y - y_off * panel.rect.w;           // top - offset * height (Y down in NDC)
    
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return panel.color;
}
