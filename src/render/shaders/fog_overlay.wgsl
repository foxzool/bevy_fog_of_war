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
@group(0) @binding(1) var visibility_texture_sampler: sampler; // Sampler for visibility & fog textures / 可见性与雾效纹理的采样器
@group(0) @binding(2) var visibility_tex: texture_2d_array<f32>;     // Current frame visibility (smooth 0-1) / 当前帧可见性 (平滑 0-1)
@group(0) @binding(3) var explored_tex: texture_2d_array<f32>;       // Explored map (0 or 1) / 已探索地图 (0 或 1)
@group(0) @binding(4) var snapshot_texture_sampler: sampler; // Sampler for snapshot texture / 快照纹理的采样器
@group(0) @binding(5) var snapshot_tex: texture_2d_array<f32>;       // Snapshot of explored areas / 已探索区域的快照
@group(0) @binding(6) var<uniform> settings: FogMapSettings;
@group(0) @binding(7) var<storage, read> chunk_mapping: array<OverlayChunkData>; // Chunk coord -> layer indices / 区块坐标 -> 层索引


// --- Constants for Blending ---
// --- 混合常量 ---
const VISIBILITY_THRESHOLD_FULLY_CLEAR: f32 = 0.95; // Visibility above this means almost no fog / 可见性高于此值意味着几乎没有雾
const VISIBILITY_THRESHOLD_START_CLEARING: f32 = 0.1; // Start fading out fog when visibility is above this / 当可见性高于此值时开始淡出雾效

const EXPLORED_SNAPSHOT_OPACITY: f32 = 0.7; // How opaque the snapshot is in explored areas / 快照在已探索区域的不透明度
                                          // You might want to make settings.fog_color_explored.a control this.
                                          // 你可能希望通过 settings.fog_color_explored.a 来控制它。

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    if (settings.enabled == 0u) {
       return vec4<f32>(0.0, 0.0, 0.0, 0.0); // Fully transparent if disabled / 如果禁用则完全透明
    }

    let screen_uv = in.uv;

    // Convert screen UV to world position
    // 将屏幕 UV 转换为世界位置
    let ndc = uv_to_ndc(screen_uv);
    let ndc_pos = vec3<f32>(ndc, 0.0);
    let world_pos = position_ndc_to_world(ndc_pos);
    let world_xy = world_pos.xy; // Use only xy for 2D comparison

    let chunk_size_f = vec2<f32>(f32(settings.chunk_size.x), f32(settings.chunk_size.y));

    // Calculate which chunk this world position falls into
    // 计算此世界位置属于哪个区块
    let chunk_coords_f = floor(world_xy / chunk_size_f);
    let chunk_coords_i = vec2<i32>(i32(chunk_coords_f.x), i32(chunk_coords_f.y));

    // Find layer indices for this chunk
    // 查找此区块的层索引
    var active_fog_layer_idx = GFX_INVALID_LAYER;
    var active_snapshot_layer_idx = GFX_INVALID_LAYER;
    var chunk_found = false;
    for (var i = 0u; i < arrayLength(&chunk_mapping); i = i + 1u) {
        let map_entry = chunk_mapping[i];
        if (map_entry.coords.x == chunk_coords_i.x && map_entry.coords.y == chunk_coords_i.y) {
            active_fog_layer_idx = map_entry.fog_layer_index; // Same layer for visibility and explored
            active_snapshot_layer_idx = map_entry.snapshot_layer_index;
            chunk_found = true;
            break;
        }
    }

    // If chunk data not found (e.g., outside GPU resident area), assume unexplored
    // 如果未找到区块数据 (例如，在 GPU 驻留区域之外)，则假定为未探索
    if (!chunk_found || active_fog_layer_idx == GFX_INVALID_LAYER) {
          return settings.fog_color_unexplored;
    }

    // Calculate UV within the specific chunk's texture
    // 计算特定区块纹理内的 UV
    let uv_in_chunk = fract(world_xy / chunk_size_f);

    // Sample visibility and explored status
    // 采样可见性和已探索状态
    // Ensure visibility_texture_sampler is Linear for smooth results from visibility_tex
    // 确保 visibility_texture_sampler 是 Linear，以便从 visibility_tex 获得平滑结果
    let current_visibility = textureSample(visibility_tex, visibility_texture_sampler, uv_in_chunk, active_fog_layer_idx).r;
    let explored_status = textureSample(explored_tex, visibility_texture_sampler, uv_in_chunk, active_fog_layer_idx).r; // Should be 0.0 or 1.0

    // --- Determine Final Fog Color ---
    // --- 确定最终雾色 ---

    if (explored_status < 0.5) { // If less than 0.5, consider it unexplored / 如果小于 0.5，则视为未探索
        return settings.fog_color_unexplored;
    }

    // At this point, the area is explored (explored_status >= 0.5)
    // 此时，该区域已探索 (explored_status >= 0.5)

    // Calculate how "clear" the vision is, for blending
    // 计算视野的“清晰”程度，用于混合
    // smoothstep(edge0, edge1, x): 0 if x < edge0, 1 if x > edge1
    let clear_factor = smoothstep(VISIBILITY_THRESHOLD_START_CLEARING, VISIBILITY_THRESHOLD_FULLY_CLEAR, current_visibility);

    if (clear_factor >= 0.999) { // Almost fully visible
        // Discarding is usually best for performance if fully clear
        // 如果完全清晰，丢弃通常对性能最好
        if (settings.vision_clear_color.a < 0.01) { // If clear color is transparent / 如果清晰颜色是透明的
             discard;
        }
        return settings.vision_clear_color; // Return configured clear color / 返回配置的清晰颜色
    }

    // Area is explored, but not fully clear. Show snapshot blended with explored fog.
    // 区域已探索，但未完全清晰。显示与已探索雾混合的快照。
    var final_color: vec4<f32>;
    if (active_snapshot_layer_idx != GFX_INVALID_LAYER) {
        let snapshot_color_sample = textureSample(snapshot_tex, snapshot_texture_sampler, uv_in_chunk, active_snapshot_layer_idx);
        // Blend snapshot with the general "explored fog" color
        // 将快照与通用的“已探索雾”颜色混合
        // fog_color_explored often has some alpha to make it semi-transparent over the snapshot
        // fog_color_explored 通常具有一些 alpha 值，使其在快照上呈半透明
        final_color = mix(snapshot_color_sample, settings.fog_color_explored, settings.fog_color_explored.a);
    } else {
        // No valid snapshot, just show explored fog
        // 没有有效的快照，仅显示已探索的雾
        final_color = settings.fog_color_explored;
    }

    // Now, fade this "explored view" towards fully clear based on `clear_factor`
    // 现在，根据 `clear_factor` 将此“已探索视图”淡化至完全清晰
    // mix(x, y, a): x if a=0, y if a=1
    // We want final_color when clear_factor=0, and vision_clear_color when clear_factor=1
    final_color = mix(final_color, settings.vision_clear_color, clear_factor);

    return final_color;
}