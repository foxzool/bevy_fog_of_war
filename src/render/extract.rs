use crate::components::*;
use crate::resources::*;
use bevy::prelude::*;
use bevy::render::Extract;
use bevy::render::render_resource::ShaderType;
use bevy::render::view::RenderLayers;
use bytemuck::{Pod, Zeroable};
use crate::render::snapshot_pass::RenderWorldSnapshotVisible;
// --- Resources in RenderWorld to hold extracted data ---
// --- RenderWorld 中用于保存提取数据的资源 ---

#[derive(Resource, Debug, Clone, Copy, Pod, Zeroable, ShaderType)]
#[repr(C)]
pub struct RenderFogMapSettings {
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
    /// 视野完全清晰区域的颜色（通常用于混合或阈值，可能完全透明）
    /// "Color" for fully visible areas (often used for blending or thresholds, might be fully transparent)
    pub vision_clear_color: Vec4,
    pub enabled: u32, // 0 for false, 1 for true
    pub _padding1: [u32; 3],
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
    pub requests: Vec<RenderWorldSnapshotRequest>,
}

// Store handles in RenderWorld too / 同样在 RenderWorld 中存储句柄
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderFogTexture(pub Handle<Image>);
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderVisibilityTexture(pub Handle<Image>);
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderSnapshotTexture(pub Handle<Image>);

// --- Data structures matching shader buffer layouts ---
// --- 与 shader 缓冲区布局匹配的数据结构 ---

// Ensure alignment and size match WGSL / 确保对齐和大小匹配 WGSL
#[derive(Copy, Clone, ShaderType, Pod, Zeroable, Debug)]
#[repr(C)]
pub struct VisionSourceData {
    pub pos: Vec2,             // World position / 世界位置
    pub range: f32,            // Vision range / 视野范围
    pub shape_type: u32,       // 0=Circle, 1=Cone, 2=Rectangle / 0=圆形, 1=扇形, 2=矩形
    pub direction: f32,        // Direction in radians / 方向（弧度）
    pub angle: f32,            // Angle in radians (for cone) / 角度（弧度，用于扇形）
    pub intensity: f32,        // Vision intensity / 视野强度
    pub transition_ratio: f32, // Transition ratio / 过渡比例
}

#[derive(Copy, Clone, ShaderType, Pod, Zeroable, Debug)]
#[repr(C)]
pub struct ChunkComputeData {
    pub coords: IVec2,        // Chunk coordinates / 区块坐标
    pub fog_layer_index: i32, // Layer index in fog texture / 雾效纹理中的层索引
    pub _padding: u32,        // WGSL IVec2/u32 alignment / WGSL IVec2/u32 对齐
}

#[derive(Copy, Clone, ShaderType, Pod, Zeroable, Debug)]
#[repr(C)]
pub struct OverlayChunkData {
    pub coords: IVec2,             // Chunk coordinates / 区块坐标
    pub fog_layer_index: i32,      // Layer index in fog texture / 雾效纹理中的层索引
    pub snapshot_layer_index: i32, // Layer index in snapshot texture / 快照纹理中的层索引
}

#[derive(Debug, Clone)]
pub struct RenderWorldSnapshotRequest {
    pub snapshot_layer_index: u32,
    pub world_bounds: Rect,
}

// --- Extraction Systems ---
// --- 提取系统 ---

pub fn extract_fog_settings(mut commands: Commands, settings: Extract<Res<FogMapSettings>>) {
    commands.insert_resource(RenderFogMapSettings {
        enabled: settings.enabled as u32,
        chunk_size: settings.chunk_size,
        texture_resolution_per_chunk: settings.texture_resolution_per_chunk,
        fog_color_unexplored: settings.fog_color_unexplored.to_linear().to_vec4(),
        fog_color_explored: settings.fog_color_explored.to_linear().to_vec4(),
        vision_clear_color: settings.vision_clear_color.to_linear().to_vec4(),
        _padding1: [0; 3],
    });
}

pub fn extract_texture_handles(
    mut commands: Commands,
    fog_texture: Extract<Res<FogTextureArray>>,
    visibility_texture: Extract<Res<VisibilityTextureArray>>,
    snapshot_texture: Extract<Res<SnapshotTextureArray>>,
) {
    // Ensure the handles exist in the RenderWorld / 确保句柄存在于 RenderWorld 中
    commands.insert_resource(RenderFogTexture(fog_texture.handle.clone()));
    commands.insert_resource(RenderVisibilityTexture(visibility_texture.handle.clone()));
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
                .map(|(transform, src)| {
                    // 将形状枚举转换为数值
                    // Convert shape enum to numeric value
                    let shape_type = match src.shape {
                        VisionShape::Circle => 0u32,
                        VisionShape::Cone => 1u32,
                        VisionShape::Square => 2u32,
                    };

                    VisionSourceData {
                        pos: transform.translation().truncate(),
                        range: src.range,
                        shape_type,
                        direction: src.direction,
                        angle: src.angle,
                        intensity: src.intensity,
                        transition_ratio: src.transition_ratio,
                    }
                }),
        );

    if sources_res.sources.is_empty() {
        sources_res.sources.push(VisionSourceData {
            pos: Default::default(),
            range: 0.0,
            shape_type: 0,
            direction: 0.0,
            angle: 0.0,
            intensity: 0.0,
            transition_ratio: 0.0,
        });
    }
}

const GFX_INVALID_LAYER: i32 = -1;
pub fn extract_gpu_chunk_data(
    mut chunk_data_res: ResMut<ExtractedGpuChunkData>,
    fog_chunk_query: Extract<Query<&FogChunk>>,
) {
    chunk_data_res.compute_chunks.clear();
    chunk_data_res.overlay_mapping.clear();

    for chunk in fog_chunk_query.iter() {
        // 只处理在 GPU 上并且具有有效层索引的区块
        // Only process chunks that are on GPU and have valid layer indices
        if chunk.state.memory_location == ChunkMemoryLocation::Gpu
            || chunk.state.memory_location == ChunkMemoryLocation::PendingCopyToGpu
        // Maybe include pending if data is already staged
        {
            let fog_idx_gfx = chunk
                .fog_layer_index
                .map_or(GFX_INVALID_LAYER, |val| val as i32);
            let snap_idx_gfx = chunk
                .snapshot_layer_index
                .map_or(GFX_INVALID_LAYER, |val| val as i32);

            chunk_data_res.compute_chunks.push(ChunkComputeData {
                coords: chunk.coords,
                fog_layer_index: fog_idx_gfx,
                _padding: 0,
            });
            chunk_data_res.overlay_mapping.push(OverlayChunkData {
                coords: chunk.coords,
                fog_layer_index: fog_idx_gfx,
                snapshot_layer_index: snap_idx_gfx,
            });
        }
    }

    // Fallback if no valid chunks found (as before)
    // 如果未找到有效区块，则回退 (同前)
    if chunk_data_res.compute_chunks.is_empty() {
        chunk_data_res.compute_chunks.push(ChunkComputeData {
            coords: Default::default(),
            fog_layer_index: -1,
            _padding: 0,
        });
        chunk_data_res.overlay_mapping.push(OverlayChunkData {
            coords: Default::default(),
            fog_layer_index: -1,
            snapshot_layer_index: -1,
        });
    }
}


/// Extracts snapshot requests from the main world to the render world.
/// 将快照请求从主世界提取到渲染世界。
pub fn extract_snapshot_requests_to_queue(
    mut commands: Commands,
    main_world_requests: Extract<Res<MainWorldSnapshotRequestQueue>>,
) {
    // We clone the requests. If there are many, consider a more efficient transfer.
    let render_requests = main_world_requests.requests.iter().map(|req| {
        RenderWorldSnapshotRequest {
            snapshot_layer_index: req.snapshot_layer_index,
            world_bounds: req.world_bounds,
            // chunk_coords: req.chunk_coords,
        }
    }).collect::<Vec<_>>();

    if !render_requests.is_empty() {
        // info!("Extracted {} snapshot requests to RenderWorld.", render_requests.len());
    }

    commands.insert_resource(SnapshotRequestQueue {
        requests: render_requests,
    });
}

/// Extracts entities with SnapshotVisible and adds RenderWorldSnapshotVisible
/// and the SNAPSHOT_RENDER_LAYER to them in the RenderWorld.
/// 提取带有 SnapshotVisible 的实体，并在 RenderWorld 中为它们添加
/// RenderWorldSnapshotVisible 和 SNAPSHOT_RENDER_LAYER。
pub fn extract_snapshot_visible_entities(
    mut commands: Commands,
    // Query for entities in the main world that have SnapshotVisible
    // Optionally include their current RenderLayers if you need to merge
    snapshot_visible_query: Extract<Query<(Entity, Option<&RenderLayers>), With<Snapshottable>>>,
) {
    for (entity, existing_layers) in snapshot_visible_query.iter() {
        let snapshot_layer = SNAPSHOT_RENDER_LAYER.clone();
        let combined_layers = match existing_layers {
            Some(layers) => layers.union(&snapshot_layer),
            None => snapshot_layer,
        };

        commands.entity(entity).insert((
            RenderWorldSnapshotVisible, // Marker for RenderWorld systems
            combined_layers,            // Ensure it's on the snapshot layer
        ));
    }
}