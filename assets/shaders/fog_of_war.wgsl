struct VertexInput {
    @location(0) position: vec3f,
    @location(1) color: vec4f,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) color: vec4f,
    @location(1) position: vec2f,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4f(model.position, 1.0);
    out.color = model.color;
    out.position = model.position.xy;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let center = vec2f(0.0, 0.0);
    let distance = length(in.position - center);
    
    // 设置内外圆的半径
    let inner_radius = 0.3;
    let outer_radius = 0.5;
    
    // 计算透明度
    let alpha = smoothstep(inner_radius, outer_radius, distance);
    
    // 返回黑色遮罩，透明度根据距离变化
    return vec4f(0.0, 0.0, 0.0, alpha);
}