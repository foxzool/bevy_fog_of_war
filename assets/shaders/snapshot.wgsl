#import bevy_render::view::View
#import bevy_pbr::view_transformations::{
  position_world_to_ndc, ndc_to_uv
}


struct ChunkArray {
    data: array<ChunkInfo>,
};

struct ChunkInfo {
    coord: vec2<i32>,
    world_min: vec2<f32>,
    world_max: vec2<f32>,
    size: vec2<u32>,
    layer_index: u32,
    // _padding: u32, // 确保与 Rust 端对齐 / Ensure alignment with Rust side
};

@group(0) @binding(0)
var<uniform> view: View;
@group(0) @binding(1)
var<storage, read> chunks: ChunkArray;
@group(0) @binding(2) var snapshot_write: texture_storage_2d_array<rgba8unorm, write>;
@group(0) @binding(3) var source_texture: texture_2d<f32>;
@group(0) @binding(4) var source_sampler: sampler;



@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dims = textureDimensions(snapshot_write);
    let pixel_coord = global_id.xy; // 快照纹理中的像素坐标
                                    // Pixel coordinate within the snapshot texture
    if (pixel_coord.x >= dims.x || pixel_coord.y >= dims.y) {
        return;
    }

    let chunk_index: u32 = global_id.z; // 当前处理的 Chunk 索引
                                        // Index of the chunk currently being processed
    // 检查 chunk_index 是否越界 (如果 chunks.data 可能为空)
    // Check if chunk_index is out of bounds (if chunks.data might be empty)
    if (chunk_index >= arrayLength(&chunks.data)) {
        return;
    }
    let chunk = chunks.data[chunk_index]; // 获取当前 Chunk 的信息
                                          // Get info for the current chunk

    // 计算此像素在 Chunk 纹理区域内的局部 UV (0.0 到 1.0)
    // Calculate the local UV of this pixel within the chunk's texture area (0.0 to 1.0)
    // 使用 chunk.size - 1 来确保 UV 映射到像素中心或边界
    // Use chunk.size - 1 to ensure UV maps correctly to pixel centers or boundaries
    let local_uv_in_chunk_texture = vec2<f32>(pixel_coord) / vec2<f32>(chunk.size - vec2<u32>(1u));

    // 根据局部 UV 计算对应的世界坐标
    // Calculate the corresponding world coordinate based on the local UV
    let world_xy = chunk.world_min + local_uv_in_chunk_texture * (chunk.world_max - chunk.world_min);

    // 使用 Bevy 函数将世界坐标转换为 NDC (可能返回 vec4)
    // Use Bevy function to convert world coordinates to NDC (likely returns vec4)
    let ndc_vec4 = position_world_to_ndc(vec3(world_xy, 0.0)); // Renamed for clarity

    // 检查是否在 NDC 范围 [-1, 1] 内 (即是否在屏幕内)
    // Check if within NDC range [-1, 1] (i.e., whether it's on screen)
    // We use the .xy components of the returned vec4
    if (abs(ndc_vec4.x) > 1.0 || abs(ndc_vec4.y) > 1.0) {
        // 在屏幕外，写入透明色
        // Outside the screen, write transparent color
        textureStore(snapshot_write, pixel_coord, chunk.layer_index, vec4<f32>(0.0));
        return;
    }

    // 3. NDC -> 源纹理 UV 坐标 [0, 1]
    // 3. NDC -> Source Texture UV Coordinates [0, 1]
    // Pass only the .xy components to ndc_to_uv
    let source_uv = ndc_to_uv(ndc_vec4.xy); // Use .xy here
    // --- 结束坐标转换 ---

    // 使用计算得到的 source_uv 从相机画面 (source_texture) 采样颜色
    // Sample color from the camera view (source_texture) using the calculated source_uv
    let sampled_color = textureSampleLevel(source_texture, source_sampler, source_uv, 0.0);

    // 将采样到的颜色写入快照纹理的对应位置和层级
    // Write the sampled color to the corresponding position and layer in the snapshot texture
    textureStore(snapshot_write, pixel_coord, chunk.layer_index, sampled_color);
}