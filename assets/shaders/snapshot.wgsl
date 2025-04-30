#import bevy_render::view::View

struct ChunkArray {
    data: array<ChunkInfo>,
};

struct ChunkInfo {
    coord: vec2<i32>,    // 区块坐标 / chunk coordinates
    world_min: vec2<f32>, // 世界空间边界最小点 / world space minimum boundary point
    world_max: vec2<f32>, // 世界空间边界最大点 / world space maximum boundary point
    size: vec2<u32>,    // 区块尺寸 / chunk size
    layer_index: u32,   // 层索引 / layer index
};

@group(0) @binding(0)
var<uniform> view: View;
@group(0) @binding(1)
var<storage, read> chunks: ChunkArray;
@group(0) @binding(2) var snapshot_write: texture_storage_2d_array<rgba8unorm, write>;
@group(0) @binding(3) var camera_color: texture_2d<f32>;
@group(0) @binding(4) var camera_sampler: sampler;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // Get the dimensions of the snapshot texture
    let dims = textureDimensions(snapshot_write);
    
    // Calculate pixel coordinates and check bounds
    let pixel_coord = global_id.xy;
    if (pixel_coord.x >= dims.x || pixel_coord.y >= dims.y) {
        return;
    }

    let chunk_index: u32 = global_id.z;
    let chunk = chunks.data[chunk_index];
    
    // Calculate local UV coordinates within the chunk
    let local_uv = vec2<f32>(pixel_coord) / vec2<f32>(chunk.size);
    let world_xy = chunk.world_min + local_uv * (chunk.world_max - chunk.world_min);
    
    // Calculate UV coordinates for sampling the camera texture
    // 计算用于采样相机纹理的UV坐标
    let camera_uv = world_xy * view.world_to_viewport.xy + view.world_to_viewport.zw;
    
    // Sample the camera color
    // 采样相机颜色
    let color = textureSample(camera_color, camera_sampler, camera_uv);
    
    // Write the camera color to the snapshot texture
    // 将相机颜色写入快照纹理
    textureStore(snapshot_write, pixel_coord, chunk.layer_index, color);
}