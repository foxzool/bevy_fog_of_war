#import bevy_render::view::View

struct VisionSourceData {
    position: vec2<f32>,
    radius: f32,
    // padding f32
};

struct ChunkComputeData {
    coords: vec2<i32>,
    fog_layer_index: i32,
    // padding u32
};


// Define FogMapSettings struct matching Rust / 定义匹配 Rust 的 FogMapSettings 结构
// Make sure fields, types, and alignment match! / 确保字段、类型和对齐匹配！
struct FogMapSettings {
    chunk_size: vec2<u32>,
    texture_resolution_per_chunk: vec2<u32>,
    fog_color_unexplored: vec4<f32>, // Assuming Color -> vec4 / 假设 Color -> vec4
    fog_color_explored: vec4<f32>,
    vision_clear_color: vec4<f32>,
    enabled: u32,
    // Add texture formats if needed by logic, otherwise padding might be needed
    // 如果逻辑需要，添加纹理格式，否则可能需要填充
    // Example padding for alignment / 对齐填充示例
     _padding2: u32,
     _padding3: u32,
     _padding4: u32,
};

const GFX_INVALID_LAYER: i32 = -1;

// Match FogMapSettings struct layout / 匹配 FogMapSettings 结构布局
// Ensure alignment and types match Rust struct / 确保对齐和类型匹配 Rust 结构
@group(0) @binding(0) var fog_texture: texture_storage_2d_array<r8unorm, read_write>; // Fog data / 雾效数据
@group(0) @binding(1) var visibility_texture: texture_storage_2d_array<r8unorm, write>; // Visibility data / 可见性数据
@group(0) @binding(2) var<storage, read> vision_sources: array<VisionSourceData>; // Vision sources / 视野源
@group(0) @binding(3) var<storage, read> chunks: array<ChunkComputeData>; // Active GPU chunks / 活动 GPU 区块
@group(0) @binding(4) var<uniform> settings: FogMapSettings; // Global settings / 全局设置


// Define workgroup size (must match dispatch size logic in Rust)
// 定义工作组大小 (必须匹配 Rust 中的分派大小逻辑)
@compute @workgroup_size(8, 8, 1) // 8x8 threads per chunk, 1 chunk per workgroup Z / 每区块 8x8 线程，每工作组 Z 1 个区块
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>, // x, y pixel within texture, z chunk index / 纹理内的 x, y 像素, z 区块索引
) {
    if (settings.enabled == 0u) {
        return;
    }
    let dims = textureDimensions(fog_texture); // Get dimensions (width, height, layers) / 获取维度 (宽, 高, 层数)
    let pixel_coord = global_id.xy; // x, y pixel within texture / 纹理内的 x, y 像素
    // Bounds check / 边界检查
    if (global_id.x >= dims.x || global_id.y >= dims.y) {
        return;
    }

    let chunk_index: u32 = global_id.z;
    let chunk = chunks[chunk_index];
    let target_layer_index = chunk.fog_layer_index;
    if (target_layer_index == GFX_INVALID_LAYER) { // Check for invalid layer
            return; // Or handle as error/default
    }

    let chunk_coords_f = vec2<f32>(f32(chunk.coords.x), f32(chunk.coords.y));
    let local_uv = vec2<f32>(pixel_coord) / vec2<f32>(settings.chunk_size);
    let chunk_size_f = vec2<f32>(f32(settings.chunk_size.x), f32(settings.chunk_size.y));
    let tex_res_f = vec2<f32>(f32(settings.texture_resolution_per_chunk.x), f32(settings.texture_resolution_per_chunk.y));
    let uv_in_chunk = (vec2<f32>(f32(global_id.x), f32(global_id.y)) + 0.5) / tex_res_f; // Texel center UV / 纹素中心 UV
    let world_pos = chunk_coords_f * chunk_size_f + uv_in_chunk * chunk_size_f;
    let world_xy = world_pos.xy;

    // Determine visibility based on vision sources
    var current_visibility: f32 = 0.0;
    // Iterate through all vision providers
    for (var i = 0u; i < arrayLength(&vision_sources); i++) {
       let vision = vision_sources[i];
       let dist = distance(world_xy, vision.position);
       if (dist < vision.radius) {
           // 使用平滑函数计算当前视野的可见性值
           // Calculate the visibility value for the current vision using a smooth function
           let visibility = select(0.0, 1.0, dist < vision.radius);

           // 使用累加混合方法替代max函数，从而避免生成明显的边界线
           // Use an accumulative blending method instead of max function to avoid creating visible boundary lines
           current_visibility = current_visibility + visibility * (1.0 - current_visibility);
       }
    }


    // 读取历史探索区域值（0~1）
    // Read the value of the historical exploration area (0~1)
    let history_value = textureLoad(fog_texture, pixel_coord, i32(target_layer_index)).x;

    // 新的历史区域值 = max(历史, 当前可见性)
    // New history value = max(history, current visibility)
    let new_history = max(history_value, current_visibility);

    // 写入新的历史区域纹理
    // Write the new history value to the history_write texture
    textureStore(fog_texture, pixel_coord, i32(target_layer_index), vec4<f32>(new_history, 0.0, 0.0, 1.0));

    // 同时写入当前帧可见性到vision_texture_write
    // Also write current frame visibility to vision_texture_write
    textureStore(visibility_texture, pixel_coord, i32(target_layer_index), vec4<f32>(current_visibility, 0.0, 0.0, 1.0));
}

// Helper for distance squared / 平方距离辅助函数
fn distanceSquared(p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    let diff = p1 - p2;
    return dot(diff, diff);
}