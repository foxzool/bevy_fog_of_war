
#import bevy_render::view::View


struct OverlayChunkData {
    coords: vec2<i32>,
    fog_layer_index: u32,
    snapshot_layer_index: u32,
};

// Match FogMapSettings struct layout / 匹配 FogMapSettings 结构布局
struct FogMapSettings {
    chunk_size: vec2<u32>,
    texture_resolution_per_chunk: vec2<u32>,
    fog_color_unexplored: vec4<f32>,
    fog_color_explored: vec4<f32>,
    vision_clear_color: vec4<f32>, // Often transparent / 通常是透明的
     _padding1: u32,
     _padding2: u32,
     _padding3: u32,
     _padding4: u32,
};

// Bindings must match layout in prepare.rs / 绑定必须匹配 prepare.rs 中的布局
@group(0) @binding(0) var<uniform> view: View; // Bevy view uniforms / Bevy 视图统一变量
@group(0) @binding(1) var fog_texture: texture_2d_array<f32>; // Sampled fog / 采样的雾效
@group(0) @binding(2) var snapshot_texture: texture_2d_array<f32>; // Sampled snapshot / 采样的快照
@group(0) @binding(3) var texture_sampler: sampler; // Sampler for snapshot / 快照的采样器
@group(0) @binding(4) var<uniform> settings: FogMapSettings; // Global settings / 全局设置
@group(0) @binding(5) var<storage, read> chunk_mapping: array<OverlayChunkData>; // Chunk coord -> layer index / 区块坐标 -> 层索引


struct FragmentInput {
    @builtin(position) position: vec4<f32>, // Clip space position / 裁剪空间位置
    @location(0) uv: vec2<f32>, // Screen UV (0-1) / 屏幕 UV (0-1)
};

// Constants for fog thresholds / 雾阈值常量
const VISIBLE_THRESHOLD: f32 = 0.1; // Allow slight fog in visible areas / 允许可见区域有轻微雾效
const EXPLORED_THRESHOLD: f32 = 0.6; // Threshold between explored and unexplored / 已探索和未探索之间的阈值
const EXPLORED_FOG_INTENSITY: f32 = 0.7; // How much to blend explored fog color / 混合多少已探索雾颜色

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    // Calculate world position from screen UV and depth (simplified for 2D)
    // 从屏幕 UV 和深度计算世界位置 (为 2D 简化)
    // Use inverse view projection / 使用逆视图投影
    let ndc = vec3<f32>(in.uv.x * 2.0 - 1.0, (1.0 - in.uv.y) * 2.0 - 1.0, 0.0); // Assuming Z=0 for 2D overlay / 假设 2D 覆盖 Z=0
    let world_h = view.inverse_view_proj * vec4<f32>(ndc, 1.0);
    let world_pos = world_h.xy / world_h.w; // Perspective divide / 透视除法

    let chunk_size_f = vec2<f32>(f32(settings.chunk_size.x), f32(settings.chunk_size.y));
    let tex_res_f = vec2<f32>(f32(settings.texture_resolution_per_chunk.x), f32(settings.texture_resolution_per_chunk.y));

    // Calculate chunk coordinates / 计算区块坐标
    let chunk_coords_f = floor(world_pos / chunk_size_f);
    let chunk_coords_i = vec2<i32>(i32(chunk_coords_f.x), i32(chunk_coords_f.y));

    // Find layer indices for this chunk using the mapping buffer
    // 使用映射缓冲区查找此区块的层索引
    var fog_layer_index = -1i; // Use signed int for "not found" / 使用有符号整数表示“未找到”
    var snapshot_layer_index = -1i;
    for (var i = 0u; i < arrayLength(&chunk_mapping); i = i + 1u) {
        if (chunk_mapping[i].coords.x == chunk_coords_i.x && chunk_mapping[i].coords.y == chunk_coords_i.y) {
            fog_layer_index = i32(chunk_mapping[i].fog_layer_index);
            snapshot_layer_index = i32(chunk_mapping[i].snapshot_layer_index);
            break;
        }
    }

    // If chunk data not found (outside GPU resident area), assume unexplored
    // 如果未找到区块数据 (在 GPU 驻留区域之外)，假设未探索
    if (fog_layer_index < 0) {
        return settings.fog_color_unexplored;
    }

    // Calculate UV within the chunk's texture / 计算区块纹理内的 UV
    let uv_in_chunk = fract(world_pos / chunk_size_f);

    // Sample fog texture (non-filterable) / 采样雾效纹理 (不可过滤)
    // Use textureSampleLevel with level 0 / 使用 level 0 的 textureSampleLevel
    let fog_value = textureSampleLevel(fog_texture, texture_sampler, vec3<f32>(uv_in_chunk, f32(fog_layer_index)), 0.0).r;

    // --- Blending Logic ---
    // --- 混合逻辑 ---

    if (fog_value <= VISIBLE_THRESHOLD) {
        // Fully visible or almost fully visible - show the underlying scene
        // 完全可见或几乎完全可见 - 显示底层场景
        discard; // Discard fragment to show scene below / 丢弃片段以显示下方场景
        // Alternative: Return clear color if blending on top / 备选: 如果在顶部混合则返回透明颜色
        // return settings.vision_clear_color;
    } else if (fog_value <= EXPLORED_THRESHOLD) {
        // Explored but not visible - show snapshot blended with explored fog
        // 已探索但不可见 - 显示与已探索雾混合的快照
        let snapshot_color = textureSample(snapshot_texture, texture_sampler, vec3<f32>(uv_in_chunk, f32(snapshot_layer_index)));

        // Optional: Desaturate or darken snapshot / 可选: 去饱和或调暗快照
        // let gray = dot(snapshot_color.rgb, vec3<f32>(0.299, 0.587, 0.114));
        // let desaturated_snapshot = vec4<f32>(vec3<f32>(gray) * 0.8, snapshot_color.a);

        // Blend snapshot with explored fog color / 将快照与已探索雾颜色混合
        let final_color = mix(snapshot_color, settings.fog_color_explored, EXPLORED_FOG_INTENSITY);
        return final_color;

    } else {
        // Unexplored - show solid unexplored fog color / 未探索 - 显示实心未探索雾颜色
        return settings.fog_color_unexplored;
    }
}