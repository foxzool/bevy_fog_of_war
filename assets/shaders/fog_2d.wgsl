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

struct FogSettings {
    chunk_size: vec2<u32>,
    fog_color: vec4<f32>,
    explored_color: vec4<f32>
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
var<uniform> fog_settings: FogSettings;

@group(0) @binding(2)
var<storage, read> visions: VisionArray;

@group(0) @binding(3)
var<storage, read> chunks: ChunkArray;

@group(0) @binding(4) var vision_texture_write: texture_storage_2d_array<r8unorm, read>;
// History exploration area read texture
@group(0) @binding(5) var history_read: texture_storage_2d_array<rgba8unorm, read>;
//@group(0) @binding(8) var snapshot_read: texture_storage_2d_array<rgba8unorm, read>;


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

    // Default fog color if outside all chunks (should ideally not happen)
    var final_color = fog_settings.fog_color;
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

            // Load visibility and history values
            let current_visibility = textureLoad(vision_texture_write, clamped_coords, i32(target_layer_index)).x;
            let history_snapshot_color = textureLoad(history_read, clamped_coords, i32(target_layer_index));
            let history_value = history_snapshot_color.a;

            // --- History Update Logic (if needed, keep or remove as necessary) ---
            // let new_history = max(history_value, current_visibility);
            // textureStore(history_write, clamped_coords, i32(target_layer_index), vec4<f32>(new_history, 0.0, 0.0, 1.0));

            // --- Calculate potential historical display color (used if history_value > 0.0) ---
            var history_display_color = history_snapshot_color; // Default transparent black if no history needed here
            if (history_value > 0.0) {
                // 使用历史区域的颜色
                // Use the color from history texture
                history_display_color = vec4<f32>(history_snapshot_color.rgb, 1.0);

                // 如果历史纹理没有颜色（全透明），则使用灰色
                // If history texture has no color (fully transparent), use gray
                if (all(history_display_color.rgb == vec3<f32>(0.0))) {
                    history_display_color = vec4<f32>(fog_settings.explored_color.rgb, 1.0);
                }
            }

            // --- Determine final color based on visibility and history ---
            if (current_visibility > 0.0) {

                final_color = vec4<f32>(fog_settings.fog_color.xyz,  1 - current_visibility);
                // Optional DEBUG overlay (apply after blending if needed)
                if (DEBUG) {
                    let index_mask = draw_layer_index_mask(world_xy, chunk);
                    // Mix the debug mask (white) onto the calculated color's RGB components
                    let mixed_rgb = mix(final_color.rgb, vec3<f32>(1.0, 1.0, 1.0), index_mask);
                    // Construct a new vec4 with the mixed RGB and original alpha
                    final_color = vec4<f32>(mixed_rgb, final_color.a);
                    // If debug mask should make it opaque: final_color.a = mix(final_color.a, 1.0, index_mask);
                }

            } else if (history_value > 0.0) {
                // Not visible, but has history: Show the historical color with fog overlay
                // 不可见但有历史：显示历史颜色并叠加半透明迷雾
                let fog_overlay = fog_settings.fog_color;
                let fog_alpha = 0.5;  // 迷雾透明度 50%
                
                // 使用 alpha 混合公式：result = source * alpha + destination * (1 - alpha)
                // Use alpha blending formula: result = source * alpha + destination * (1 - alpha)
                final_color = vec4<f32>(
                    fog_overlay.rgb * fog_alpha + history_display_color.rgb * (1.0 - fog_alpha),
                    1.0  // 保持完全不透明 / Keep fully opaque
                );
            } else {
                // Not visible, no history: Full fog
                final_color = fog_settings.fog_color;
            }

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