use crate::components::*;
use crate::resources::*;
use bevy::prelude::*;
use bevy::render::Extract;
use bevy::render::render_resource::ShaderType;
use bytemuck::{Pod, Zeroable};
// --- Resources in RenderWorld to hold extracted data ---
// --- RenderWorld 中用于保存提取数据的资源 ---

#[derive(Resource, Debug, Clone, Copy, Pod, Zeroable, ShaderType)]
#[repr(C)]
pub struct GpuFogMapSettings {
    /// 每个区块的大小 (世界单位)
    /// Size of each chunk (in world units)
    pub chunk_size: UVec2,
    /// 每个区块对应纹理的分辨率 (像素)
    /// Resolution of the texture per chunk (in pixels)
    pub texture_resolution_per_chunk: UVec2,
    /// 未探索区域的雾颜色
    /// Fog color for unexplored areas
    pub fog_color_unexplored: Vec4, // Use Vec4 for shader compatibility / 使用 Vec4 以兼容 shader
    /// 已探索但当前不可见区域的雾颜色 (通常是半透明)
    /// Fog color for explored but not currently visible areas (usually semi-transparent)
    pub fog_color_explored: Vec4,
    /// 视野完全清晰区域的“颜色”（通常用于混合或阈值，可能完全透明）
    /// "Color" for fully visible areas (often used for blending or thresholds, might be fully transparent)
    pub vision_clear_color: Vec4,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ExtractedVisionSources {
    // Store data in a format suitable for GPU buffer / 以适合 GPU 缓冲区的格式存储数据
    pub sources: Vec<VisionSourceData>,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ExtractedGpuChunkData {
    // Store data needed by compute and overlay shaders / 存储 compute 和 overlay shader 所需的数据
    pub compute_chunks: Vec<ChunkComputeData>,
    pub overlay_mapping: Vec<OverlayChunkData>, // For overlay lookup / 用于覆盖查找
}

#[derive(Resource, Debug, Clone, Default)]
pub struct SnapshotRequestQueue {
    // Chunks needing snapshot this frame / 本帧需要快照的区块
    pub requests: Vec<SnapshotRequest>,
}

// Store handles in RenderWorld too / 同样在 RenderWorld 中存储句柄
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderFogTexture(pub Handle<Image>);
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderSnapshotTexture(pub Handle<Image>);

// --- Data structures matching shader buffer layouts ---
// --- 与 shader 缓冲区布局匹配的数据结构 ---

// Ensure alignment and size match WGSL / 确保对齐和大小匹配 WGSL
#[derive(Copy, Clone, ShaderType, Pod, Zeroable, Debug)]
#[repr(C)]
pub struct VisionSourceData {
    pub pos: Vec2,     // World position / 世界位置
    pub range_sq: f32, // Use squared range for efficiency / 使用平方范围以提高效率
    pub _padding: f32, // WGSL vec2/f32 alignment / WGSL vec2/f32 对齐
}

#[derive(Copy, Clone, ShaderType, Pod, Zeroable, Debug)]
#[repr(C)]
pub struct ChunkComputeData {
    pub coords: IVec2,        // Chunk coordinates / 区块坐标
    pub fog_layer_index: u32, // Layer index in fog texture / 雾效纹理中的层索引
    pub _padding: u32,        // WGSL IVec2/u32 alignment / WGSL IVec2/u32 对齐
}

#[derive(Copy, Clone, ShaderType, Pod, Zeroable, Debug)]
#[repr(C)]
pub struct OverlayChunkData {
    pub coords: IVec2,             // Chunk coordinates / 区块坐标
    pub fog_layer_index: u32,      // Layer index in fog texture / 雾效纹理中的层索引
    pub snapshot_layer_index: u32, // Layer index in snapshot texture / 快照纹理中的层索引
}

#[derive(Debug, Clone)]
pub struct SnapshotRequest {
    pub snapshot_layer_index: u32,
    pub world_bounds: Rect,
    pub chunk_coords: IVec2, // Needed for filtering entities / 用于过滤实体
}

// --- Extraction Systems ---
// --- 提取系统 ---

pub fn extract_fog_settings(mut commands: Commands, settings: Extract<Res<FogMapSettings>>) {
    commands.insert_resource(GpuFogMapSettings {
        chunk_size: settings.chunk_size,
        texture_resolution_per_chunk: settings.texture_resolution_per_chunk,
        fog_color_unexplored: settings.fog_color_unexplored.to_linear().to_vec4(),
        fog_color_explored: settings.fog_color_explored.to_linear().to_vec4(),
        vision_clear_color: settings.vision_clear_color.to_linear().to_vec4(),
    });
}

pub fn extract_texture_handles(
    mut commands: Commands,
    fog_texture: Extract<Res<FogTextureArray>>,
    snapshot_texture: Extract<Res<SnapshotTextureArray>>,
) {
    // Ensure the handles exist in the RenderWorld / 确保句柄存在于 RenderWorld 中
    commands.insert_resource(RenderFogTexture(fog_texture.handle.clone()));
    commands.insert_resource(RenderSnapshotTexture(snapshot_texture.handle.clone()));
}

pub fn extract_vision_sources(
    mut sources_res: ResMut<ExtractedVisionSources>,
    vision_sources: Extract<Query<(&GlobalTransform, &VisionSource)>>,
) {
    sources_res.sources.clear();
    sources_res
        .sources
        .extend(
            vision_sources
                .iter()
                .filter(|(_, src)| src.enabled)
                .map(|(transform, src)| VisionSourceData {
                    pos: transform.translation().truncate(),
                    range_sq: src.range * src.range,
                    _padding: 0.0,
                }),
        );
}

pub fn extract_gpu_chunk_data(
    mut chunk_data_res: ResMut<ExtractedGpuChunkData>,
    cache: Extract<Res<ChunkStateCache>>,
    chunk_manager: Extract<Res<ChunkEntityManager>>,
    chunk_q: Extract<Query<&FogChunk>>,
) {
    chunk_data_res.compute_chunks.clear();
    chunk_data_res.overlay_mapping.clear();

    for coords in &cache.gpu_resident_chunks {
        if let Some(entity) = chunk_manager.map.get(coords) {
            if let Ok(chunk) = chunk_q.get(*entity) {
                // Add data for compute shader / 为 compute shader 添加数据
                chunk_data_res.compute_chunks.push(ChunkComputeData {
                    coords: chunk.coords,
                    fog_layer_index: chunk.fog_layer_index,
                    _padding: 0,
                });
                // Add data for overlay shader mapping / 为 overlay shader 映射添加数据
                chunk_data_res.overlay_mapping.push(OverlayChunkData {
                    coords: chunk.coords,
                    fog_layer_index: chunk.fog_layer_index,
                    snapshot_layer_index: chunk.snapshot_layer_index,
                });
            }
        }
    }
}

// This system simulates the result of `prepare_snapshot_render_data`
// It should ideally run *after* that system in the main world schedule
// or directly extract the resource populated by it.
// 这个系统模拟 `prepare_snapshot_render_data` 的结果
// 理想情况下，它应该在主世界调度中的那个系统 *之后* 运行
// 或者直接提取由它填充的资源。
pub fn extract_snapshot_requests(
    mut queue_res: ResMut<SnapshotRequestQueue>,
    // Assuming a resource `MainWorldSnapshotQueue` is populated in the main world
    // main_world_queue: Extract<Res<MainWorldSnapshotQueue>>,
    // OR recalculate based on cache (less ideal)
    // 或基于缓存重新计算 (不太理想)
    cache: Extract<Res<ChunkStateCache>>,
    chunk_manager: Extract<Res<ChunkEntityManager>>,
    chunk_q: Extract<Query<&FogChunk>>,
) {
    queue_res.requests.clear();
    // Strategy: Snapshot visible chunks currently on GPU / 策略: 快照当前在 GPU 上的可见区块
    for coords in &cache.visible_chunks {
        if cache.gpu_resident_chunks.contains(coords) {
            if let Some(entity) = chunk_manager.map.get(coords) {
                if let Ok(chunk) = chunk_q.get(*entity) {
                    queue_res.requests.push(SnapshotRequest {
                        snapshot_layer_index: chunk.snapshot_layer_index,
                        world_bounds: chunk.world_bounds,
                        chunk_coords: chunk.coords,
                    });
                }
            }
        }
    }
}
