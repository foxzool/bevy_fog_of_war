#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_render::view::View
#import bevy_pbr::view_transformations::{
    uv_to_ndc,
    position_ndc_to_world,
}

struct OverlayChunkData { // Used for mapping world coord to layer index in textures / 用于将世界坐标映射到纹理中的层索引
    coords: vec2<i32>,
    fog_layer_index: i32,        // Layer index for fog_texture (explored) and visibility_texture / fog_texture (已探索) 和 visibility_texture 的层索引
    snapshot_layer_index: i32, // Layer index for snapshot_texture / snapshot_texture 的层索引
    // padding u32,
};

struct FogMapSettings {
    chunk_size: vec2<u32>,
    texture_resolution_per_chunk: vec2<u32>,
    fog_color_unexplored: vec4<f32>,
    fog_color_explored: vec4<f32>,
    vision_clear_color: vec4<f32>, // Usually (0,0,0,0) for full transparency / 通常是 (0,0,0,0) 以实现完全透明
    enabled: u32,
    _padding2: u32,
    _padding3: u32,
    _padding4: u32,
};

const GFX_INVALID_LAYER: i32 = -1;

// --- Bindings for fog_overlay ---
// --- fog_overlay 的绑定 ---
@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var fog_sampler: sampler; // Sampler for visibility & fog textures / 可见性与雾效纹理的采样器
@group(0) @binding(2) var visibility_tex: texture_2d_array<f32>;     // Current frame visibility (smooth 0-1) / 当前帧可见性 (平滑 0-1)
@group(0) @binding(3) var explored_tex: texture_2d_array<f32>;       // Explored map (0 or 1) / 已探索地图 (0 或 1)
@group(0) @binding(4) var snapshot_tex: texture_2d_array<f32>;       // Snapshot of explored areas / 已探索区域的快照
@group(0) @binding(5) var<uniform> settings: FogMapSettings;
@group(0) @binding(6) var<storage, read> chunk_mapping: array<OverlayChunkData>; // Chunk coord -> layer indices / 区块坐标 -> 层索引


// --- Constants for Blending ---
// --- 混合常量 ---
const VISIBILITY_THRESHOLD_FULLY_CLEAR: f32 = 0.95; // Visibility above this means almost no fog / 可见性高于此值意味着几乎没有雾
const VISIBILITY_THRESHOLD_START_CLEARING: f32 = 0.1; // Start fading out fog when visibility is above this / 当可见性高于此值时开始淡出雾效

// Controls the softness of the transition between UNEXPLORED and EXPLORED states.
// 控制未探索 (UNEXPLORED) 和已探索 (EXPLORED) 状态之间过渡的柔和度。
// explored_tex contains values around 0.0 (unexplored) or 1.0 (explored).
// Linear sampling will create values between 0 and 1 at the edges.
// This width defines how much of that interpolated range is used for the smooth transition.
// A larger value means a softer, wider edge. 0.05 means transition from ~0.475 to ~0.525 if centered at 0.5
// 较大的值意味着更柔和、更宽的边缘。0.05 表示如果以 0.5 为中心，则从约 0.475 过渡到约 0.525
const EXPLORED_TRANSITION_WIDTH: f32 = 0.12; // Adjust this value (e.g., 0.05 to 0.2) / 调整此值 (例如，0.05 到 0.2)
// The center point of the transition. Since explored_tex is 0 or 1,
// linear sampling will make the boundary average around 0.5.
// 过渡的中心点。由于 explored_tex 是 0 或 1，线性采样将使边界平均值在 0.5 左右。
const EXPLORED_TRANSITION_CENTER: f32 = 0.5;
 
@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    if (settings.enabled == 0u) {
       return vec4<f32>(0.0, 0.0, 0.0, 0.0); // Fully transparent if disabled / 如果禁用则完全透明
    }

    let screen_uv = in.uv;
    
    let ndc = uv_to_ndc(screen_uv);
    let ndc_pos = vec3<f32>(ndc, 0.0);
    let world_pos = position_ndc_to_world(ndc_pos);
    let world_pos_xy = world_pos.xy; // Use only xy for 2D comparison

    let chunk_size_f = vec2<f32>(f32(settings.chunk_size.x), f32(settings.chunk_size.y));
    let chunk_coords_f = floor(world_pos_xy / chunk_size_f);
    let chunk_coords_i = vec2<i32>(i32(chunk_coords_f.x), i32(chunk_coords_f.y));

    var active_fog_layer_idx = GFX_INVALID_LAYER;
    var active_snapshot_layer_idx = GFX_INVALID_LAYER;
    var chunk_found = false;
    for (var i = 0u; i < arrayLength(&chunk_mapping); i = i + 1u) {
        let map_entry = chunk_mapping[i];
        if (map_entry.coords.x == chunk_coords_i.x && map_entry.coords.y == chunk_coords_i.y) {
            active_fog_layer_idx = map_entry.fog_layer_index;
            active_snapshot_layer_idx = map_entry.snapshot_layer_index;
            chunk_found = true;
            break;
        }
    }

    if (!chunk_found || active_fog_layer_idx == GFX_INVALID_LAYER) {
          // For areas outside mapped chunks, smoothly transition to unexplored based on distance if desired,
          // or just return unexplored for simplicity.
          // 对于映射区块之外的区域，如果需要，可以根据距离平滑过渡到未探索状态，或者为简单起见直接返回未探索。
          return settings.fog_color_unexplored;
    }
    let uv_in_chunk = fract(world_pos_xy / chunk_size_f);


    // Sample visibility and explored status using LINEAR filtering for smooth transitions
    // 使用线性过滤采样可见性和已探索状态，以实现平滑过渡
    let current_visibility = textureSample(visibility_tex, fog_sampler, uv_in_chunk, active_fog_layer_idx).r;
    let explored_value_raw = textureSample(explored_tex, fog_sampler, uv_in_chunk, active_fog_layer_idx).r; // Value is 0.0 to 1.0 due to linear sampling / 由于线性采样，值在 0.0 到 1.0 之间

    // --- Smooth transition from Unexplored to Explored ---
    // --- 从未探索平滑过渡到已探索 ---
    // alpha_explored will be 0.0 for fully unexplored, 1.0 for fully explored.
    // alpha_explored 对于完全未探索将是 0.0，对于完全已探索将是 1.0。
    let edge0 = EXPLORED_TRANSITION_CENTER - EXPLORED_TRANSITION_WIDTH / 2.0;
    let edge1 = EXPLORED_TRANSITION_CENTER + EXPLORED_TRANSITION_WIDTH / 2.0;
    let alpha_explored = smoothstep(edge0, edge1, explored_value_raw);

    // If almost fully unexplored, return unexplored color directly.
    // 如果几乎完全未探索，则直接返回未探索颜色。
    if (alpha_explored < 0.001) {
        return settings.fog_color_unexplored;
    }

    // --- Logic for areas that are at least partially explored ---
    // --- 至少部分探索区域的逻辑 ---

    // Calculate how "clear" the vision is for currently visible areas
    // 计算当前可见区域的视野“清晰”程度
    let clear_factor = smoothstep(VISIBILITY_THRESHOLD_START_CLEARING, VISIBILITY_THRESHOLD_FULLY_CLEAR, current_visibility);

    var explored_content_color: vec4<f32>;
    if (active_snapshot_layer_idx != GFX_INVALID_LAYER) {
        let snapshot_color_sample = textureSample(snapshot_tex, fog_sampler, uv_in_chunk, active_snapshot_layer_idx);
        // Mix snapshot with the semi-transparent explored fog color
        // 将快照与半透明的已探索雾色混合
        explored_content_color = mix(snapshot_color_sample, settings.fog_color_explored, settings.fog_color_explored.a);
    } else {
        explored_content_color = settings.fog_color_explored; // No snapshot, just explored fog
    }

    // If currently visible, fade the explored_content_color towards fully clear (or settings.vision_clear_color)
    // 如果当前可见，则将 explored_content_color 淡化至完全清晰 (或 settings.vision_clear_color)
    // mix(x,y,a): x if a=0, y if a=1.
    // We want explored_content_color if clear_factor = 0 (not visible now)
    // We want vision_clear_color if clear_factor = 1 (fully visible now)
    let visible_or_explored_color = mix(explored_content_color, settings.vision_clear_color, clear_factor);

    // If clear_factor is high enough and vision_clear_color is transparent, discard for performance.
    // 如果 clear_factor 足够高且 vision_clear_color 是透明的，则为性能而丢弃。
    if (clear_factor > 0.99 && settings.vision_clear_color.a < 0.01) {
        discard;
    }

    // Finally, blend between settings.fog_color_unexplored and visible_or_explored_color using alpha_explored
    // 最后，使用 alpha_explored 在 settings.fog_color_unexplored 和 visible_or_explored_color 之间混合
    // This creates the smooth edge between the "fully unexplored" and "anything that has been explored" states.
    // 这会在“完全未探索”和“任何已探索过”的状态之间创建平滑的边缘。
    let final_color = mix(settings.fog_color_unexplored, visible_or_explored_color, alpha_explored);

    return final_color;
}