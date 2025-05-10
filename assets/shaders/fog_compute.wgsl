#import bevy_render::view::View
struct VisionSourceData {
    position: vec2<f32>,
    radius: f32,
    // padding f32
};

struct ChunkComputeData {
    coords: vec2<i32>,
    fog_layer_index: i32, // Assuming this index is valid for BOTH fog_texture and visibility_texture layers / 假设此索引对 fog_texture 和 visibility_texture 的层都有效
    // padding u32
};

struct FogMapSettings {
    chunk_size: vec2<u32>, // World size of a chunk / 区块的世界大小
    texture_resolution_per_chunk: vec2<u32>, // Texture pixels per chunk / 每区块的纹理像素
    fog_color_unexplored: vec4<f32>,
    fog_color_explored: vec4<f32>,
    vision_clear_color: vec4<f32>,
    enabled: u32,
    _padding2: u32,
    _padding3: u32,
    _padding4: u32,
};

const GFX_INVALID_LAYER: i32 = -1;
const VISION_TRANSITION_RATIO: f32 = 0.20; // 20% of radius for smooth fade / 半径的 20% 用于平滑淡出
const EXPLORATION_VISIBILITY_THRESHOLD: f32 = 0.05; // How much visibility is needed to mark as explored / 标记为已探索需要多少可见度

@group(0) @binding(0) var fog_texture: texture_storage_2d_array<r8unorm, read_write>; // Stores explored status (0.0 = unexplored, 1.0 = explored) / 存储已探索状态 (0.0 = 未探索, 1.0 = 已探索)
@group(0) @binding(1) var visibility_texture: texture_storage_2d_array<r8unorm, write>; // Stores current frame visibility (0.0 = not visible, 1.0 = fully visible) / 存储当前帧可见性 (0.0 = 不可见, 1.0 = 完全可见)
@group(0) @binding(2) var<storage, read> vision_sources: array<VisionSourceData>;
@group(0) @binding(3) var<storage, read> chunks: array<ChunkComputeData>;
@group(0) @binding(4) var<uniform> settings: FogMapSettings;

@compute @workgroup_size(8, 8, 1)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>, // global_id.xy is pixel coord within the chunk's texture area / global_id.xy 是区块纹理区域内的像素坐标
) {
    if (settings.enabled == 0u) {
        // Optionally clear visibility texture if disabled, or just do nothing
        // 如果禁用，可选择清除可见性纹理，或什么都不做
        // textureStore(visibility_texture, vec2<i32>(global_id.xy), i32(global_id.z), vec4<f32>(0.0)); // Might need layer mapping
        return;
    }

    let chunk_index = global_id.z;
    // Assuming global_id.z directly maps to an index in the chunks array
    // 假设 global_id.z 直接映射到 chunks 数组中的索引
    if (chunk_index >= arrayLength(&chunks)) { return; } // Bounds check for chunk_index / chunk_index 边界检查

    let chunk_data = chunks[chunk_index];
    let target_layer_idx = chunk_data.fog_layer_index;

    if (target_layer_idx == GFX_INVALID_LAYER) {
        return;
    }

    // pixel_coord_in_chunk is global_id.xy, assuming workgroup processes one chunk
    // 假设工作组处理一个区块，pixel_coord_in_chunk 是 global_id.xy
    let pixel_coord_in_chunk = vec2<i32>(i32(global_id.x), i32(global_id.y));

    // Bounds check for pixel coordinates within the chunk's texture resolution
    // 区块纹理分辨率内的像素坐标边界检查
    if (global_id.x >= settings.texture_resolution_per_chunk.x || global_id.y >= settings.texture_resolution_per_chunk.y) {
        return;
    }

    let chunk_world_origin = vec2<f32>(f32(chunk_data.coords.x), f32(chunk_data.coords.y)) * vec2<f32>(f32(settings.chunk_size.x), f32(settings.chunk_size.y));
    let tex_res_f = vec2<f32>(f32(settings.texture_resolution_per_chunk.x), f32(settings.texture_resolution_per_chunk.y));
    let chunk_size_f = vec2<f32>(f32(settings.chunk_size.x), f32(settings.chunk_size.y));

    // UV within the current chunk's texture portion (0.0 to 1.0 range)
    // 当前区块纹理部分内的 UV (0.0 到 1.0 范围)
    let uv_in_chunk = (vec2<f32>(global_id.xy) + 0.5) / tex_res_f;

    // World position of the current texel
    // 当前纹素的世界位置
    let world_pos_xy = chunk_world_origin + uv_in_chunk * chunk_size_f;

    // --- Calculate Current Visibility ---
    // --- 计算当前可见性 ---
    var current_visibility: f32 = 0.0;
    for (var i = 0u; i < arrayLength(&vision_sources); i = i + 1u) {
       let source = vision_sources[i];
       let dist = distance(world_pos_xy, source.position);

       let inner_radius = source.radius * (1.0 - VISION_TRANSITION_RATIO);
       let single_source_visibility = smoothstep(source.radius, inner_radius, dist); // 1.0 if dist <= inner_radius, 0.0 if dist >= source.radius

       // Accumulative blending for multiple vision sources
       // 多个视野源的累积混合
       current_visibility = current_visibility + single_source_visibility * (1.0 - current_visibility);
       // Optimization: if current_visibility is already 1.0, no need to check more sources
       // 优化: 如果 current_visibility 已经是 1.0，则无需检查更多源
       if (current_visibility >= 0.999) {
           current_visibility = 1.0;
           break;
       }
    }
    // Store current visibility
    // 存储当前可见性
    textureStore(visibility_texture, pixel_coord_in_chunk, target_layer_idx, vec4<f32>(current_visibility, 0.0, 0.0, 1.0));


    // --- Update Explored Map (fog_texture) ---
    // --- 更新已探索地图 (fog_texture) ---
    let previous_explored_value = textureLoad(fog_texture, pixel_coord_in_chunk, target_layer_idx).r;
    var new_explored_value = previous_explored_value;

    if (current_visibility > EXPLORATION_VISIBILITY_THRESHOLD) {
        // If currently visible enough, mark as fully explored (1.0)
        // This ensures explored areas are definitively marked.
        // 如果当前足够可见，则标记为完全探索 (1.0)
        // 这确保了已探索区域被明确标记。
        new_explored_value = 1.0;
        // Alternative: allow explored areas to "fade" if not re-seen for a while (more complex)
        // 备选: 如果一段时间未重新看到，则允许已探索区域“褪色”(更复杂)
        // new_explored_value = max(previous_explored_value, current_visibility); // If you want explored to reflect max visibility ever seen
    }
    // Store updated explored status
    // 存储更新的已探索状态
    textureStore(fog_texture, pixel_coord_in_chunk, target_layer_idx, vec4<f32>(new_explored_value, 0.0, 0.0, 1.0));
}