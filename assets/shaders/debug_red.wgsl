// 迷雾效果着色器 - 高级实时迷雾
// Fog effect shader - advanced real-time fog

// 迷雾设置结构
// Fog settings structure
struct FogSettings {
    color: vec4<f32>,       // 迷雾颜色
    center: vec2<f32>,     // 迷雾中心位置
    density: f32,          // 迷雾密度
    range: f32,            // 迷雾范围
    time: f32,             // 时间（用于动画）
    clear_radius: f32,     // 相机周围的透明半径 / clear radius around camera
    clear_falloff: f32,    // 边缘过渡效果 / edge falloff effect
    _padding3: f32,        // 填充 / padding
};

@group(0) @binding(2)
var<uniform> fog_settings: FogSettings;

// 顶点着色器输出结构
// Vertex shader output structure
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// 顶点着色器
// Vertex shader
@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    
    // 创建一个全屏三角形
    // Create a full-screen triangle
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0)
    );
    
    // 对应的UV坐标
    // Corresponding UV coordinates
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
        vec2<f32>(0.0, 2.0),
    );
    
    out.position = vec4<f32>(pos[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    
    return out;
}

// 输入纹理和采样器
// Input texture and sampler
@group(0) @binding(0)
var input_texture: texture_2d<f32>;
@group(0) @binding(1)
var input_sampler: sampler;

// 噪声函数，用于生成随机效果
// Noise function for random effects
fn noise(p: vec2<f32>) -> f32 {
    let pi = vec2<f32>(13.9898, 78.233);
    return fract(sin(dot(p, pi)) * 43758.5453);
}

// 改进的噪声函数，生成更自然的效果
// Improved noise function for more natural effects
fn improved_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    
    // 四个角落的随机值
    // Four corner random values
    let a = noise(i);
    let b = noise(i + vec2<f32>(1.0, 0.0));
    let c = noise(i + vec2<f32>(0.0, 1.0));
    let d = noise(i + vec2<f32>(1.0, 1.0));
    
    // 平滑插值
    // Smooth interpolation
    let u = f * f * (3.0 - 2.0 * f);
    
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// 片段着色器
// Fragment shader
@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // 从输入纹理采样场景颜色
    // Sample scene color from input texture
    let scene_color = textureSample(input_texture, input_sampler, in.uv);
    
    // 计算到迷雾中心的距离
    // Calculate distance to fog center
    // 使用 UV 坐标转换为屏幕坐标
    // Convert UV coordinates to screen position
    let screen_position = in.uv * 2.0 - 1.0;
    
    // 计算到中心的距离，使用横纵比调整成椒形
    // Calculate distance to center, adjust for aspect ratio to make it elliptical
    let aspect_ratio = 1.78; // 16:9 屏幕宽高比 / 16:9 aspect ratio
    let adjusted_position = vec2<f32>(screen_position.x * aspect_ratio, screen_position.y);
    // 直接使用简单的距离计算，不进行额外的缩放
    // Use simple distance calculation without additional scaling
    let distance_to_center = length(adjusted_position - fog_settings.center);
    
    // 计算相机周围的透明区域
    // Calculate clear area around camera
    let clear_radius = fog_settings.clear_radius;
    let clear_falloff = fog_settings.clear_falloff;
    
    // 计算透明区域的边缘过渡
    // Calculate the edge transition of the clear area
    // 当距离小于 (clear_radius - clear_falloff) 时完全透明 (1.0)
    // 当距离大于 clear_radius 时完全不透明 (0.0)
    // Fully transparent (1.0) when distance < (clear_radius - clear_falloff)
    // Fully opaque (0.0) when distance > clear_radius
    let in_clear_area = 1.0 - smoothstep(clear_radius - clear_falloff, clear_radius, distance_to_center);
    
    // 调试输出 - 如果透明度因子大于 0.5，使用红色标记
    // Debug output - if clear factor > 0.5, mark with red
    // if in_clear_area > 0.5 {
    //     return vec4<f32>(1.0, 0.0, 0.0, 1.0);
    // }
    
    // 基于距离计算迷雾强度，使用指数减弱更自然
    // Calculate fog intensity based on distance, using exponential falloff for more natural look
    // 注意这里的公式变化，使得距离越远迷雾越强
    // Note the formula change to make fog stronger with distance
    let distance_factor = 1.0 - exp(-distance_to_center * 0.8 / fog_settings.range);
    
    // 使用改进的噪声函数生成多层噪声
    // Use improved noise function to generate multi-layered noise
    let time_scale = fog_settings.time * 0.1;
    
    // 第一层噪声 - 大尺度结构
    // First noise layer - large scale structure
    let noise1 = improved_noise(screen_position * 0.3 + vec2<f32>(time_scale * 0.2, time_scale * 0.1)) * 0.5;
    
    // 第二层噪声 - 中等细节
    // Second noise layer - medium details
    let noise2 = improved_noise(screen_position * 0.7 + vec2<f32>(-time_scale * 0.15, time_scale * 0.2)) * 0.3;
    
    // 第三层噪声 - 小细节
    // Third noise layer - small details
    let noise3 = improved_noise(screen_position * 1.5 + vec2<f32>(time_scale * 0.3, -time_scale * 0.25)) * 0.2;
    
    // 综合噪声
    // Combined noise
    let noise_value = noise1 + noise2 + noise3;
    
    // 添加时间变化使迷雾呼吸
    // Add time variation to make fog breathe
    let time_factor = (sin(fog_settings.time * 0.1) * 0.5 + 0.5) * 0.15;
    
    // 计算最终迷雾强度，使用更自然的混合
    // Calculate final fog intensity with more natural blending
    let base_intensity = distance_factor * fog_settings.density;
    let noise_influence = mix(0.7, 1.3, noise_value + time_factor);
    
    // 计算最终的迷雾强度，考虑透明区域
    // Calculate final fog intensity considering clear area
    // 在透明区域内降低迷雾强度
    // Reduce fog intensity inside clear area
    let fog_intensity = base_intensity * noise_influence * (1.0 - in_clear_area);
    
    // 计算迷雾颜色，根据距离进行色调变化
    // Calculate fog color with distance-based hue variation
    let base_fog_color = fog_settings.color;
    
    // 根据噪声稍微调整颜色以增加变化
    // Slightly adjust color based on noise for variation
    let color_variation = 0.05 * (noise_value - 0.5);
    let adjusted_fog_color = vec4<f32>(
        base_fog_color.r + color_variation,
        base_fog_color.g + color_variation * 0.5,
        base_fog_color.b + color_variation * 0.7,
        base_fog_color.a
    );
    
    // 在相机周围的透明区域处理
    // Handle the clear area around camera
    if in_clear_area > 0.01 {
        // 在边缘区域使用混合，在完全透明区域直接返回原始场景
        // Use blending in edge area, directly return original scene in completely clear area
        if in_clear_area > 0.99 {
            return scene_color;
        } else {
            // 在边缘区域使用混合
            // Use blending in edge area
            let edge_blend = in_clear_area;
            return mix(fog_result, scene_color, edge_blend);
        }
    }
    
    // 在迷雾区域使用正常的混合模式
    // Use normal blending mode in fog area
    let fog_result = mix(scene_color, adjusted_fog_color, clamp(fog_intensity, 0.0, 1.0));
    
    // 添加轻微的光晕效果
    // Add subtle bloom effect
    let bloom_factor = 0.02 * fog_intensity;
    let bloom_color = adjusted_fog_color + vec4<f32>(0.2, 0.1, 0.1, 0.0);
    let final_color = mix(fog_result, bloom_color, bloom_factor);
    
    return final_color;
}
