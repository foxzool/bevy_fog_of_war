#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_render::view::View
#import bevy_pbr::view_transformations::{
    uv_to_ndc,
    position_ndc_to_world,
}

const DEBUG: bool = false;

// Represents a single vision source.// 代表单个视野源。
struct VisionSource {
    position: vec2<f32>, // World position of the vision source / 视野源的世界坐标 (8 bytes, offset 0)
    radius: f32,         // Vision radius / 视野半径 (4 bytes, offset 8)
    falloff: f32,
};

// Contains all vision sources.
// 包含所有视野源。
struct VisionArray {
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

struct MetaUniform {
    chunks_per_row: u32,
    chunk_size: u32
};

// 迷雾设置结构
// Fog settings structure
struct FogMaterial {
    color: vec4<f32>,       // 迷雾颜色 / fog color
};

// Chunk信息数组
// Chunk information array
struct ChunkArray {
    data: array<ChunkInfo>,
};

@group(0) @binding(0)
var<uniform> view: View;

@group(0) @binding(1)
var<uniform> fog_material: FogMaterial;

@group(0) @binding(2)
var<storage, read> visions: VisionArray;

@group(0) @binding(3)
var<storage, read> chunks: ChunkArray;

@group(0) @binding(4) var vision_texture_write: texture_storage_2d_array<r8unorm, read>;
@group(0) @binding(5) var<uniform> chunk_meta: MetaUniform;
// History exploration area read texture
@group(0) @binding(6) var history_read: texture_storage_2d_array<r8unorm, read>;
// History exploration area write texture
@group(0) @binding(7) var history_write: texture_storage_2d_array<r8unorm, write>;


@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Calculate world position from UV coordinates
    // Screen UV to View UV
    // Convert screen UV to view UV, taking the viewport into account.
    let uv = in.uv;
    // Use helper functions to convert UV to world position
    let ndc = uv_to_ndc(uv);
    let ndc_pos = vec3<f32>(ndc, 0.0);
    let world_pos = position_ndc_to_world(ndc_pos);
    let world_xy = world_pos.xy; // Use only xy for 2D comparison
  
   // Determine visibility based on vision sources
    var current_visibility: f32 = 1.0;



    // Default fog color if outside all chunks (should ideally not happen)
    var final_color = fog_material.color; 
    var found_chunk = false;

    // Cache chunk count for loop
    let chunk_count = arrayLength(&chunks.data);
    // Find the chunk containing the current world position
    for (var i = 0u; i < chunk_count; i = i + 1u) {
        let chunk = chunks.data[i];
        // Check if the world position is within the chunk boundaries
        if (world_xy.x >= chunk.world_min.x && world_xy.x < chunk.world_max.x &&
            world_xy.y >= chunk.world_min.y && world_xy.y < chunk.world_max.y) {

            let target_layer_index = chunk.layer_index;

             // Calculate relative position within the chunk (0.0 to 1.0 range)
            let rel_pos_norm = (world_xy - chunk.world_min) / (chunk.world_max - chunk.world_min);

            // Map normalized position to integer texture coordinates
            // Assuming the vision texture size for this layer matches the chunk size
            let tex_coords_raw = vec2<i32>(floor(rel_pos_norm * vec2<f32>(chunk.size)));
            let clamped_coords = clamp(tex_coords_raw, vec2<i32>(0), vec2<i32>(chunk.size) - vec2<i32>(1));
            current_visibility  = textureLoad(vision_texture_write, clamped_coords, i32(target_layer_index)).x;

            // 读取历史探索区域值（0~1）
            // Read the value of the historical exploration area (0~1)
            let history_value = textureLoad(history_read, clamped_coords, i32(target_layer_index)).x;
//            let history_value = 0.0;

            // 新的历史区域值 = max(历史, 当前可见性)
            // New history value = max(history, current visibility)
            let new_history = max(history_value, current_visibility);

            // 写入新的历史区域纹理
            // Write the new history value to the history_write texture
//            textureStore(history_write, clamped_coords, i32(target_layer_index), vec4<f32>(new_history, 0.0, 0.0, 1.0));

            // 同时写入当前帧可见性到vision_texture_write
            // Also write current frame visibility to vision_texture_write
//            textureStore(vision_texture_write, clamped_coords, i32(target_layer_index), vec4<f32>(current_visibility, 0.0, 0.0, 1.0));

            let visibility = current_visibility;

            var alpha = 1.0 ;

            if (visibility > 0.0) {
              alpha =  1 - visibility;
            } else if (new_history > 0.0) {
              alpha =  0.5;
            }

            var color_rgb = fog_material.color.xyz;
            // DEBUG: overlay layer index on fog color
            if (DEBUG) {
                let index_mask = draw_layer_index_mask(world_xy, chunk);
                color_rgb = mix(color_rgb, vec3<f32>(1.0, 1.0, 1.0), index_mask);
            }
            final_color = vec4<f32>(color_rgb, alpha);

            found_chunk = true;
            break; // Found the correct chunk, exit loop
        }
    }

    // If no chunk was found (which indicates a potential issue in setup or coordinates),
    // handle it gracefully. Returning a bright color like magenta can help debugging.
    if (!found_chunk) {
        // Pixel outside known chunks; fallback to default fog color
        return final_color;
    }

    // Return the final determined color and alpha
    // The color is the fog color for visible areas.
    return final_color;
}



fn rect(pt: vec2<f32>, center: vec2<f32>, size: vec2<f32>) -> f32 {
    return step(center.x - size.x * 0.5, pt.x) * step(pt.x, center.x + size.x * 0.5)
         * step(center.y - size.y * 0.5, pt.y) * step(pt.y, center.y + size.y * 0.5);
}

fn draw_digit_mask(pt: vec2<f32>, center: vec2<f32>, pattern: u32, size: vec2<f32>, thickness: f32) -> f32 {
    var m: f32 = 0.0;
    if ((pattern & 0x01u) != 0u) {
        let seg_center = center + vec2<f32>(0.0, size.y * 0.5 - thickness * 0.5);
        m = m + rect(pt, seg_center, vec2<f32>(size.x, thickness));
    }
    if ((pattern & 0x02u) != 0u) {
        let seg_center = center + vec2<f32>(size.x * 0.5 - thickness * 0.5, size.y * 0.25);
        m = m + rect(pt, seg_center, vec2<f32>(thickness, size.y * 0.5));
    }
    if ((pattern & 0x04u) != 0u) {
        let seg_center = center + vec2<f32>(size.x * 0.5 - thickness * 0.5, -size.y * 0.25);
        m = m + rect(pt, seg_center, vec2<f32>(thickness, size.y * 0.5));
    }
    if ((pattern & 0x08u) != 0u) {
        let seg_center = center + vec2<f32>(0.0, -size.y * 0.5 + thickness * 0.5);
        m = m + rect(pt, seg_center, vec2<f32>(size.x, thickness));
    }
    if ((pattern & 0x10u) != 0u) {
        let seg_center = center + vec2<f32>(-size.x * 0.5 + thickness * 0.5, -size.y * 0.25);
        m = m + rect(pt, seg_center, vec2<f32>(thickness, size.y * 0.5));
    }
    if ((pattern & 0x20u) != 0u) {
        let seg_center = center + vec2<f32>(-size.x * 0.5 + thickness * 0.5, size.y * 0.25);
        m = m + rect(pt, seg_center, vec2<f32>(thickness, size.y * 0.5));
    }
    if ((pattern & 0x40u) != 0u) {
        let seg_center = center + vec2<f32>(0.0, 0.0);
        m = m + rect(pt, seg_center, vec2<f32>(size.x, thickness));
    }
    return clamp(m, 0.0, 1.0);
}

fn draw_layer_index_mask(pt: vec2<f32>, chunk: ChunkInfo) -> f32 {
    let idx = chunk.layer_index;
    var digits: u32 = 1u;
    if (idx >= 100u) {
        digits = 3u;
    } else if (idx >= 10u) {
        digits = 2u;
    }
    let size_base = min(chunk.size.x, chunk.size.y);
    let base = f32(size_base) * 0.2;
    let thickness = base * 0.15;
    let spacing = base * 1.2;
    let patterns = array<u32, 10>(0x3Fu, 0x06u, 0x5Bu, 0x4Fu, 0x66u, 0x6Du, 0x7Du, 0x07u, 0x7Fu, 0x6Fu);
    var m: f32 = 0.0;
    let offset_start = - (f32(digits - 1u) * spacing * 0.5);
    for (var i: u32 = 0u; i < digits; i = i + 1u) {
        var divisor: u32 = 1u;
        if (digits == 3u) {
            if (i == 0u) {
                divisor = 100u;
            } else if (i == 1u) {
                divisor = 10u;
            }
        } else if (digits == 2u) {
            if (i == 0u) {
                divisor = 10u;
            }
        }
        let d = (idx / divisor) % 10u;
        let pat = patterns[d];
        let digit_center = (chunk.world_min + chunk.world_max) * 0.5 + vec2<f32>(offset_start + f32(i) * spacing, 0.0);
        m = m + draw_digit_mask(pt, digit_center, pat, vec2<f32>(base, base), thickness);
    }
    return clamp(m, 0.0, 1.0);
}