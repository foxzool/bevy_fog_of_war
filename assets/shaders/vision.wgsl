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
    size: vec2<u32>,    // 区块尺寸 / chunk size
    layer_index: u32,   // 层索引 / layer index
};

// Chunk信息数组
// Chunk information array
struct ChunkArray {
    data: array<ChunkInfo>,
};

struct MetaUniform {
    chunks_per_row: u32,
    chunk_size: u32
};

// --- Bind Group 0 ---
@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<storage, read> visions: VisionParams;
@group(0) @binding(2) var<storage, read> chunks: ChunkArray;
@group(0) @binding(3) var vision_texture_write: texture_storage_2d_array<r8unorm, write>;
@group(0) @binding(4) var<uniform> chunk_meta: MetaUniform;
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
    let chunk_size = chunk_meta.chunk_size;
    let chunk_x = global_id.xy.x / chunk_size;
    let chunk_y = global_id.xy.y / chunk_size;
    let chunk_index: u32 = global_id.z;
    let chunk = chunks.data[chunk_index];
    let target_layer_index: u32 = chunk.layer_index;
    let local_uv = vec2<f32>(pixel_coord) / vec2<f32>(chunk.size);
    let world_xy = chunk.world_min + local_uv * (chunk.world_max - chunk.world_min);

    // Determine visibility based on vision sources
    var current_visibility: f32 = 0.0;
    // Iterate through all vision providers
    for (var i = 0u; i < arrayLength(&visions.sources); i++) {
       let vision = visions.sources[i];
       // Use squared distance for performance, avoid sqrt initially
       // 使用平方距离代替开方，提升性能
       let dist_sq = dot(world_xy - vision.position, world_xy - vision.position);
       let radius_sq = vision.radius * vision.radius;

       var visibility: f32 = 0.0; // Visibility for this source

       if (dist_sq < radius_sq) {
           // Clamp falloff factor between 0.0 and 1.0
           // 将衰减因子限制在 0.0 和 1.0 之间
           let falloff_factor = clamp(vision.falloff, 0.0, 1.0);

           // Calculate the radius where falloff starts (full visibility radius)
           // 计算衰减开始的半径（完全可见半径）
           let falloff_start_radius = vision.radius * (1.0 - falloff_factor);

           // Avoid issues if falloff_start_radius is negative (though unlikely with clamp)
           // 避免 falloff_start_radius 为负数（虽然 clamp 后不太可能）
           if (falloff_start_radius <= 0.0) {
               // If falloff covers the entire radius or more, calculate falloff from center
               // 如果衰减覆盖整个半径或更多，则从中心开始计算衰减
               let dist = sqrt(dist_sq);
               // Use smoothstep for smooth transition from 1.0 (center) to 0.0 (edge)
               // 使用 smoothstep 实现从 1.0（中心）到 0.0（边缘）的平滑过渡
               visibility = smoothstep(vision.radius, 0.0, dist);
           } else {
               let falloff_start_radius_sq = falloff_start_radius * falloff_start_radius;
               if (dist_sq <= falloff_start_radius_sq) {
                   // Inside the full visibility radius
                   // 在完全可见半径内
                   visibility = 1.0;
               } else {
                   // Inside the falloff zone, calculate smooth transition
                   // 在衰减区域内，计算平滑过渡
                   let dist = sqrt(dist_sq);
                   // smoothstep(edge0, edge1, x) interpolates from 0 to 1 as x goes from edge0 to edge1.
                   // We want 1.0 at falloff_start_radius and 0.0 at vision.radius.
                   // smoothstep(edge0, edge1, x) 当 x 从 edge0 到 edge1 时，插值从 0 到 1。
                   // 我们希望在 falloff_start_radius 处为 1.0，在 vision.radius 处为 0.0。
                   visibility = smoothstep(vision.radius, falloff_start_radius, dist);
               }
           }
       }
       // else: visibility remains 0.0 (outside radius) / 否则：可见性保持为 0.0（在半径之外）

       // Combine with overall visibility using the original blending method
       // 使用原始混合方法与总体可见性结合
       current_visibility = current_visibility + visibility * (1.0 - current_visibility);

       // Optimization: break early if max visibility reached
       // 优化：如果达到最大可见性，则提前中断
       if (current_visibility > 0.999) {
          break;
      }
    }


    // 读取历史探索区域值（0~1）
    // Read the value of the historical exploration area (0~1)
    let history_value = textureLoad(history_read, pixel_coord, i32(target_layer_index)).x;

    // Convert world position to NDC
    // 将世界坐标转换为 NDC
    let ndc_vec4 = position_world_to_ndc(vec3(world_xy, 0.0));

    // Initialize color to transparent black
    // 初始化颜色为透明黑色
    var sampled_color = vec4<f32>(0.0);

    // Check if within NDC range [-1, 1] (i.e., whether it's on screen)
    // 检查是否在 NDC 范围 [-1, 1] 内（即是否在屏幕内）
    if (abs(ndc_vec4.x) <= 1.0 && abs(ndc_vec4.y) <= 1.0) {
        // Convert NDC to UV coordinates
        // 将 NDC 转换为 UV 坐标
        let source_uv = ndc_to_uv(ndc_vec4.xy);
        sampled_color = textureSampleLevel(source_texture, source_sampler, source_uv, 0.0);
    }

    // 新的历史区域值 = max(历史, 当前可见性)
    // New history value = max(history, current visibility)
    let new_history = max(history_value, current_visibility);

    // 写入新的历史区域纹理，如果当前区域可见则写入采样的颜色，否则保持历史记录
    // Write to history texture: if area is visible write sampled color, otherwise keep history
    let final_color = select(vec4<f32>(new_history), sampled_color, current_visibility > 0.0);
    textureStore(history_write, pixel_coord, i32(target_layer_index), final_color);

    // 同时写入当前帧可见性到vision_texture_write
    // Also write current frame visibility to vision_texture_write
    textureStore(vision_texture_write, pixel_coord, i32(target_layer_index), vec4<f32>(current_visibility, 0.0, 0.0, 1.0));
}
