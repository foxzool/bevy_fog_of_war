// 迷雾着色器
// Fog shader

// 顶点着色器
// Vertex shader
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    
    // 创建一个全屏四边形
    // Create a full-screen quad
    let x = f32(vertex_index & 1u) * 2.0 - 1.0;
    let y = f32((vertex_index >> 1u) & 1u) * 2.0 - 1.0;
    
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    
    return out;
}

// 片段着色器
// Fragment shader
struct FogSettings {
    // 迷雾颜色 (默认为黑色)
    // Fog color (default to black)
    color: vec4<f32>,
    // 迷雾密度
    // Fog density
    density: f32,
    // 填充
    // Padding
    _padding1: f32,
    _padding2: f32,
    _padding3: f32,
    // 相机位置
    // Camera position
    camera_position: vec2<f32>,
    // 迷雾范围
    // Fog range
    fog_range: f32,
    // 迷雾最大强度
    // Maximum fog intensity
    max_intensity: f32,
}

@group(0) @binding(0)
var screen_texture: texture_2d<f32>;
@group(0) @binding(1)
var texture_sampler: sampler;
@group(0) @binding(2)
var<uniform> settings: FogSettings;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // 获取原始颜色
    // Get the original color
    let original_color = textureSample(screen_texture, texture_sampler, in.uv);
    
    // 计算世界坐标
    // Calculate world coordinates
    let world_pos = vec2<f32>(
        (in.uv.x - 0.5) * 2.0 * settings.fog_range + settings.camera_position.x,
        (in.uv.y - 0.5) * 2.0 * settings.fog_range + settings.camera_position.y
    );
    
    // 计算到相机的距离
    // Calculate distance to camera
    let distance_to_camera = length(world_pos - settings.camera_position);
    
    // 计算迷雾强度
    // Calculate fog intensity
    let fog_factor = min(
        settings.max_intensity,
        1.0 - exp(-distance_to_camera * settings.density)
    );
    
    // 混合原始颜色和迷雾颜色 (默认为黑色)
    // Blend original color and fog color (default to black)
    return mix(original_color, settings.color, fog_factor);
}
