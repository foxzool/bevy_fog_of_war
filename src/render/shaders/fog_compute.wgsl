#import bevy_render::view::View

struct VisionSourceData {
    pos: vec2<f32>,
    range: f32,
    // padding f32
};

struct ChunkComputeData {
    coords: vec2<i32>,
    fog_layer_index: i32,
    // padding u32
};

const GFX_INVALID_LAYER: i32 = -1;

// Match FogMapSettings struct layout / 匹配 FogMapSettings 结构布局
// Ensure alignment and types match Rust struct / 确保对齐和类型匹配 Rust 结构
@group(0) @binding(0) var fog_texture: texture_storage_2d_array<r8unorm, read_write>; // Fog data / 雾效数据
@group(0) @binding(1) var<storage, read> vision_sources: array<VisionSourceData>; // Vision sources / 视野源
@group(0) @binding(2) var<storage, read> chunks: array<ChunkComputeData>; // Active GPU chunks / 活动 GPU 区块
@group(0) @binding(3) var<uniform> settings: FogMapSettings; // Global settings / 全局设置

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


// Define constants for fog values / 定义雾值的常量
const FOG_VISIBLE: f32 = 0.0;
const FOG_EXPLORED: f32 = 0.5; // Example value / 示例值
const FOG_UNEXPLORED: f32 = 1.0;

// Define workgroup size (must match dispatch size logic in Rust)
// 定义工作组大小 (必须匹配 Rust 中的分派大小逻辑)
@compute @workgroup_size(8, 8, 1) // 8x8 threads per chunk, 1 chunk per workgroup Z / 每区块 8x8 线程，每工作组 Z 1 个区块
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>, // x, y pixel within texture, z chunk index / 纹理内的 x, y 像素, z 区块索引
) {
    if (settings.enabled == 0u) {
        return;
    }
    let texture_dims = textureDimensions(fog_texture); // Get dimensions (width, height, layers) / 获取维度 (宽, 高, 层数)
    let chunk_index = global_id.z;

    // Bounds check / 边界检查
    if (global_id.x >= texture_dims.x || global_id.y >= texture_dims.y) {
        return;
    }

    // Get chunk info for this invocation / 获取此调用的区块信息
    let chunk_data = chunks[chunk_index];
    let fog_layer_idx = chunk_data.fog_layer_index;

    if (fog_layer_idx == GFX_INVALID_LAYER) { // Check for invalid layer
        return; // Or handle as error/default
    }

    let chunk_coords_f = vec2<f32>(f32(chunk_data.coords.x), f32(chunk_data.coords.y));
    let fog_layer_index = chunk_data.fog_layer_index;

    // Calculate texture coordinates within the layer / 计算层内的纹理坐标
    let texel_coord = vec2<i32>(i32(global_id.x), i32(global_id.y));

    // Calculate world position of this texel / 计算此纹素的世界位置
    let chunk_size_f = vec2<f32>(f32(settings.chunk_size.x), f32(settings.chunk_size.y));
    let tex_res_f = vec2<f32>(f32(settings.texture_resolution_per_chunk.x), f32(settings.texture_resolution_per_chunk.y));
    let uv_in_chunk = (vec2<f32>(f32(global_id.x), f32(global_id.y)) + 0.5) / tex_res_f; // Texel center UV / 纹素中心 UV
    let world_pos = chunk_coords_f * chunk_size_f + uv_in_chunk * chunk_size_f;

    // Load current fog value / 加载当前雾值
    // Use vec4<f32> because textureLoad only works with formats like rgba8uint etc.
    // R8Unorm needs careful handling or use a different format like R32Float if easier.
    // 使用 vec4<f32> 因为 textureLoad 仅适用于 rgba8uint 等格式。
    // R8Unorm 需要小心处理，或者如果更容易，使用不同的格式如 R32Float。
    // Let's assume R8Unorm maps directly to the 'r' component.
    // 假设 R8Unorm 直接映射到 'r' 组件。
    let current_fog_vec = textureLoad(fog_texture, texel_coord, i32(fog_layer_index));
    let current_fog = current_fog_vec.r; // Extract the single float value / 提取单个浮点值

    // Check against vision sources / 对照视野源检查
    var is_visible = false;
    for (var i = 0u; i < arrayLength(&vision_sources); i = i + 1u) {
        let source = vision_sources[i];
        let dist_sq = distanceSquared(world_pos, source.pos);
        if (dist_sq <= source.range * source.range) {
            is_visible = true;
            break;
        }
    }

    // Determine new fog state / 确定新的雾状态
    var new_fog = current_fog;
    if (is_visible) {
        new_fog = FOG_VISIBLE;
    } else {
        // If not visible, but was previously revealed (not unexplored)
        // 如果不可见，但之前被揭示过 (不是未探索)
        if (current_fog < FOG_UNEXPLORED) {
             new_fog = FOG_EXPLORED;
        }
        // Otherwise, it stays unexplored / 否则，它保持未探索
    }

    // Write the new fog value back / 写回新的雾值
    // Write back as vec4<f32> for r8unorm storage texture / 作为 vec4<f32> 写回 r8unorm 存储纹理
    textureStore(fog_texture, texel_coord, i32(fog_layer_index), vec4<f32>(new_fog, 0.0, 0.0, 1.0));
}

// Helper for distance squared / 平方距离辅助函数
fn distanceSquared(p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    let diff = p1 - p2;
    return dot(diff, diff);
}