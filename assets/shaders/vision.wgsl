#import bevy_render::view::View
#import bevy_pbr::view_transformations::{position_world_to_ndc, ndc_to_uv}

// Represents a single vision source.// 代表单个视野源。
struct VisionSource {
    position: vec2<f32>, // World position of the vision source / 视野源的世界坐标 (8 bytes, offset 0)
    radius: f32,         // Vision radius / 视野半径 (4 bytes, offset 8)
    falloff: f32,
};

// Contains all vision sources.
// 包含所有视野源。
struct VisionParams {
    sources: array<VisionSource>, // Array of vision sources, stride 16 / 视野源数组，步幅 16
};

// Chunk信息结构体
// Chunk information structure
struct ChunkInfo {
    coord: vec2<i32>,    // 区块坐标 / chunk coordinates
    world_min: vec2<f32>, // 世界空间边界最小点 / world space minimum boundary point
    world_max: vec2<f32>, // 世界空间边界最大点 / world space maximum boundary point
    layer_index: u32,   // 层索引 / layer index
};

// Chunk信息数组
// Chunk information array
struct ChunkArray {
    data: array<ChunkInfo>,
};

struct FogSettings {
    chunk_size: vec2<u32>,
    fog_color: vec4<f32>,
    explored_color: vec4<f32>
};

// --- Bind Group 0 ---
@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<storage, read> visions: VisionParams;
@group(0) @binding(2) var<storage, read> chunks: ChunkArray;
@group(0) @binding(3) var vision_texture_write: texture_storage_2d_array<r8unorm, write>;
@group(0) @binding(4) var<uniform> fog_settings: FogSettings;
// History exploration area read texture
@group(0) @binding(5) var history_read: texture_storage_2d_array<rgba8unorm, read>;
// History exploration area write texture
@group(0) @binding(6) var history_write: texture_storage_2d_array<rgba8unorm, write>;
@group(0) @binding(7) var source_texture: texture_2d<f32>;
@group(0) @binding(8) var source_sampler: sampler;



// --- Compute Shader ---

// Workgroup size for processing the texture array
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dims = textureDimensions(vision_texture_write);
    // Calculate pixel coordinates and check bounds
    let pixel_coord = global_id.xy;
    if (pixel_coord.x >= dims.x || pixel_coord.y >= dims.y) {
        return;
    }
    let chunk_size = fog_settings.chunk_size;
    let chunk_x = global_id.xy.x / chunk_size;
    let chunk_y = global_id.xy.y / chunk_size;
    let chunk_index: u32 = global_id.z;
    let chunk = chunks.data[chunk_index];
    let target_layer_index: u32 = chunk.layer_index;
    let local_uv = vec2<f32>(pixel_coord) / vec2<f32>(chunk_size);
    let world_xy = chunk.world_min + local_uv * (chunk.world_max - chunk.world_min);

    // Determine visibility based on vision sources
    var current_visibility: f32 = 0.0;
    // Iterate through all vision providers
    for (var i = 0u; i < arrayLength(&visions.sources); i++) {
       let vision = visions.sources[i];
       // Use squared distance for performance, avoid sqrt initially
       // 使用平方距离代替开方，提升性能
       // 计算像素到圆心的距离
       // Calculate distance from pixel to circle center
       let pixel_to_center = world_xy - vision.position;
       let dist = length(pixel_to_center);
       let radius = vision.radius;
       let falloff = vision.falloff; // 获取 falloff 值

       // 计算像素大小用于抗锯齿 (注释掉)
       // Calculate pixel size for anti-aliasing
       // let pixel_size = length((chunk.world_max - chunk.world_min) / vec2<f32>(chunk.size));

       // 使用单个像素大小的抗锯齿 (注释掉)
       // Anti-alias using single pixel size
       // let aa_range = pixel_size * 0.5;

       // 计算可见度 - 使用更宽的 falloff 范围
       // Calculate visibility - using a wider falloff range
       // 当 dist <= radius - falloff 时，值为 1.0
       // 当 dist >= radius + falloff 时，值为 0.0
       // 在 radius - falloff 和 radius + falloff 之间平滑过渡
       var visibility = smoothstep(radius + falloff, radius - falloff, dist);

       // Combine with overall visibility using the original blending method
       // 使用原始混合方法与总体可见性结合
       current_visibility = current_visibility + visibility * (1.0 - current_visibility);

       // Optimization: break early if max visibility reached
       // 优化：如果达到最大可见性，则提前中断
       if (current_visibility > 0.999) {
          break;
      }
    }

    // Clamp final visibility just to be safe, although blending should handle it.
    // 为安全起见，钳制最终可见性（尽管混合应该能处理大于1的情况）。
    current_visibility = clamp(current_visibility, 0.0, 1.0);

    // Define a small threshold for visibility checks
    // 为可见性检查定义一个小的阈值
    let visibility_threshold = 0.01; // 你可能需要调整这个值

    // 读取历史探索区域值（0~1）
    // Read the value of the historical exploration area (0~1)
    let history_value = textureLoad(history_read, pixel_coord, i32(target_layer_index)).x;

    // Convert world position to NDC
    // 将世界坐标转换为 NDC
    let ndc_vec4 = position_world_to_ndc(vec3(world_xy, 0.0));
    let source_uv = ndc_to_uv(ndc_vec4.xy);

    var final_history_color: vec4<f32>;
    let history_texture = textureLoad(history_read, pixel_coord, i32(target_layer_index));

    // Use the threshold for the visibility check
    // 使用阈值进行可见性检查
    if (current_visibility > visibility_threshold) {
        // 当前可见：采样场景颜色并存储
        // Currently visible: sample and store scene color
        if (abs(ndc_vec4.x) <= 1.0 && abs(ndc_vec4.y) <= 1.0) {
            // 在屏幕内，采样场景颜色
            // Inside screen, sample scene color
            let sampled_color = textureSampleLevel(source_texture, source_sampler, source_uv, 0.0);
            final_history_color = vec4<f32>(sampled_color.rgb, 1.0);
        } else {
            // 在屏幕外，但是可见 - 保持当前历史颜色
            // Outside screen but visible - keep current history color
            final_history_color = history_texture;
        }
    } else if (history_texture.a > 0.0) {
        // 当前不可见但有历史记录：保持历史颜色
        // Not visible but has history: keep history color
        final_history_color = history_texture;
    } else {
        // 既不可见也没有历史：存储透明色
        // Neither visible nor has history: store transparent
        final_history_color = vec4<f32>(0.0);
    }

    textureStore(history_write, pixel_coord, i32(target_layer_index), final_history_color);

    // 同时写入当前帧可见性到vision_texture_write
    // Also write current frame visibility to vision_texture_write
    // The clamped value is already used here
    textureStore(vision_texture_write, pixel_coord, i32(target_layer_index), vec4<f32>(current_visibility, 0.0, 0.0, 1.0));
}
