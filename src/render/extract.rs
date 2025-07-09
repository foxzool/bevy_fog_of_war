#![allow(dead_code)]

use crate::prelude::*;
use bevy::render::Extract;
use bevy::render::render_resource::ShaderType;
use bytemuck::{Pod, Zeroable};

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

// Store handles in RenderWorld too / 同样在 RenderWorld 中存储句柄
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderFogTexture(pub Handle<Image>);
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderVisibilityTexture(pub Handle<Image>);
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderSnapshotTexture(pub Handle<Image>);

#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderSnapshotTempTexture(pub Handle<Image>);

// --- Data structures matching shader buffer layouts ---
// --- 与 shader 缓冲区布局匹配的数据结构 ---

// Ensure alignment and size match WGSL / 确保对齐和大小匹配 WGSL
#[derive(Copy, Clone, ShaderType, Pod, Zeroable, Debug)]
#[repr(C)]
pub struct VisionSourceData {
    pub position: Vec2,        // World position / 世界位置 (matches WGSL `position`)
    pub radius: f32,           // Vision range / 视野范围 (matches WGSL `radius`)
    pub shape_type: u32, // 0=Circle, 1=Cone, 2=Rectangle / 0=圆形, 1=扇形, 2=矩形 (matches WGSL `shape_type`)
    pub direction_rad: f32, // Direction in radians / 方向（弧度） (matches WGSL `direction`)
    pub angle_rad: f32, // Angle in radians (for cone) / 角度（弧度，用于扇形） (matches WGSL `angle`)
    pub intensity: f32, // Vision intensity / 视野强度 (matches WGSL `intensity`)
    pub transition_ratio: f32, // Transition ratio / 过渡比例 (matches WGSL `transition_ratio`)

    // --- Precalculated values for WGSL --- / --- 为 WGSL 预计算的值 ---
    pub cos_direction: f32,       // cos(direction_rad)
    pub sin_direction: f32,       // sin(direction_rad)
    pub cone_half_angle_cos: f32, // cos(angle_rad * 0.5)

    pub _padding1: f32, // Padding to match WGSL struct size (48 bytes) / 填充以匹配 WGSL 结构体大小 (48 字节)
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
    snapshot_temp_texture: Extract<Res<SnapshotTempTexture>>,
) {
    // Ensure the handles exist in the RenderWorld / 确保句柄存在于 RenderWorld 中
    commands.insert_resource(RenderFogTexture(fog_texture.handle.clone()));
    commands.insert_resource(RenderVisibilityTexture(visibility_texture.handle.clone()));
    commands.insert_resource(RenderSnapshotTexture(snapshot_texture.handle.clone()));
    commands.insert_resource(RenderSnapshotTempTexture(
        snapshot_temp_texture.handle.clone(),
    ));
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

                    let cos_dir = src.direction.cos();
                    let sin_dir = src.direction.sin();
                    // For cone, angle is the full FOV. Shader uses half_angle.
                    // 对于扇形，angle 是完整的视场角。Shader 使用半角。
                    let cone_cos_half_angle = (src.angle * 0.5).cos();

                    VisionSourceData {
                        position: transform.translation().truncate(),
                        radius: src.range,
                        shape_type,
                        direction_rad: src.direction, // Store original direction in radians / 存储原始方向（弧度）
                        angle_rad: src.angle, // Store original angle in radians / 存储原始角度（弧度）
                        intensity: src.intensity,
                        transition_ratio: src.transition_ratio,
                        cos_direction: cos_dir,
                        sin_direction: sin_dir,
                        cone_half_angle_cos: cone_cos_half_angle,
                        _padding1: 0.0, // Initialize padding / 初始化填充
                    }
                }),
        );

    if sources_res.sources.is_empty() {
        sources_res.sources.push(VisionSourceData {
            position: Default::default(),
            radius: 0.0,
            shape_type: 0, // Circle by default / 默认为圆形
            direction_rad: 0.0,
            angle_rad: 0.0, // Full circle if cone, but irrelevant for shape_type 0 / 如果是扇形则为全圆，但对 shape_type 0 无关紧要
            intensity: 0.0,
            transition_ratio: 0.0,
            cos_direction: 1.0,       // cos(0)
            sin_direction: 0.0,       // sin(0)
            cone_half_angle_cos: 1.0, // cos(0 * 0.5)
            _padding1: 0.0,
        });
    }
}

const GFX_INVALID_LAYER: i32 = -1;

pub fn extract_gpu_chunk_data(
    mut chunk_data_res: ResMut<ExtractedGpuChunkData>,
    settings: Extract<Res<FogMapSettings>>,
    camera_query: Extract<Query<(&GlobalTransform, &Projection), With<FogOfWarCamera>>>,
    fog_chunk_query: Extract<Query<&FogChunk>>,
) {
    chunk_data_res.compute_chunks.clear();
    chunk_data_res.overlay_mapping.clear();

    let mut view_aabb_world: Option<Rect> = None;

    if let Ok((camera_transform, projection)) = camera_query.single() {
        // Calculate view AABB for an orthographic camera
        // This assumes the FogOfWarCamera is orthographic. Handle perspective if needed.
        if let Projection::Orthographic(ortho_projection) = projection {
            let camera_scale = camera_transform.compute_transform().scale;
            // ortho_projection.area gives the size of the projection area.
            // For WindowSize scale mode, this area is in logical pixels, needing viewport size.
            // For Fixed scale mode, this area is in world units.
            // We'll assume Fixed scale mode or that area is already in appropriate units
            // that can be scaled by camera_transform.scale to get world dimensions.
            // A more robust way for WindowSize would be to use camera.logical_viewport_size().
            let half_width = ortho_projection.area.width() * 0.5 * camera_scale.x;
            let half_height = ortho_projection.area.height() * 0.5 * camera_scale.y;
            let camera_pos_2d = camera_transform.translation().truncate();

            view_aabb_world = Some(Rect {
                min: Vec2::new(camera_pos_2d.x - half_width, camera_pos_2d.y - half_height),
                max: Vec2::new(camera_pos_2d.x + half_width, camera_pos_2d.y + half_height),
            });
        } else {
            warn!(
                "FogOfWarCamera is not using an OrthographicProjection. Culling might not work as expected for perspective cameras with this AABB logic."
            );
            // For perspective, you'd need to implement frustum culling.
        }
    } else {
        warn!(
            "No single FogOfWarCamera found, or multiple were found. Fog chunk culling will not be performed."
        );
        // If no camera, all GPU-ready chunks will be processed (original behavior for this path)
    }

    let chunk_world_size_f32 = settings.chunk_size.as_vec2();

    for chunk in fog_chunk_query.iter() {
        if !(chunk.state.memory_location == ChunkMemoryLocation::Gpu
            || chunk.state.memory_location == ChunkMemoryLocation::PendingCopyToGpu)
        {
            continue; // Skip if not on GPU or pending
        }

        let chunk_min_world = chunk.coords.as_vec2() * chunk_world_size_f32;
        let chunk_max_world = chunk_min_world + chunk_world_size_f32;
        let chunk_aabb_world = Rect {
            min: chunk_min_world,
            max: chunk_max_world,
        };

        let mut is_visible_or_no_culling = true; // Default to true if culling is not active
        if let Some(view_rect) = view_aabb_world {
            // AABB intersection test
            is_visible_or_no_culling = !(chunk_aabb_world.max.x < view_rect.min.x
                || chunk_aabb_world.min.x > view_rect.max.x
                || chunk_aabb_world.max.y < view_rect.min.y
                || chunk_aabb_world.min.y > view_rect.max.y);
        }

        if is_visible_or_no_culling {
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

    // Fallback if no valid chunks found (or all culled)
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
