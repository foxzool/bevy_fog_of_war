use crate::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor, TextureFormatPixelInfo};
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureUsages};
use std::fmt::Display;

/// 摄像机组件
/// Fog of war camera component
#[derive(Component)]
pub struct FogOfWarCamera;

/// 视野源组件
/// Vision source component
#[derive(Component, Reflect, ExtractComponent, Clone)]
#[reflect(Component)]
pub struct VisionSource {
    /// 视野范围（世界单位）
    /// Vision range (world units)
    pub range: f32,
    /// 是否启用
    /// Enabled
    pub enabled: bool,
    /// 视野形状（默认为圆形）
    /// Vision shape (default is circle)
    pub shape: VisionShape,
    /// 视野方向（弧度，仅在扇形视野时使用）
    /// Vision direction (radians, only used for cone vision)
    pub direction: f32,
    /// 视野角度（弧度，仅在扇形视野时使用）
    /// Vision angle (radians, only used for cone vision)
    pub angle: f32,
    /// 视野强度（影响可见性计算）
    /// Vision intensity (affects visibility calculation)
    pub intensity: f32,
    /// 视野过渡比例（从完全可见到不可见的过渡区域占总半径的比例）
    /// Vision transition ratio (ratio of total radius for transition from fully visible to not visible)
    pub transition_ratio: f32,
}

impl VisionSource {
    pub fn circle(range: f32) -> Self {
        Self {
            range,
            enabled: true,
            shape: VisionShape::Circle,
            direction: 0.0,
            angle: std::f32::consts::FRAC_PI_2,
            intensity: 1.0,
            transition_ratio: 0.2,
        }
    }

    pub fn cone(range: f32, direction: f32, angle: f32) -> Self {
        Self {
            range,
            enabled: true,
            shape: VisionShape::Cone,
            direction,
            angle,
            intensity: 1.0,
            transition_ratio: 0.2,
        }
    }

    pub fn square(range: f32) -> Self {
        Self {
            range,
            enabled: true,
            shape: VisionShape::Square,
            direction: 0.0,
            angle: std::f32::consts::FRAC_PI_2,
            intensity: 1.0,
            transition_ratio: 0.2,
        }
    }
}

/// 视野形状
/// Vision shape
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Default)]
pub enum VisionShape {
    /// 圆形视野（全方向）
    /// Circular vision (omnidirectional)
    #[default]
    Circle,
    /// 扇形视野（有方向和角度）
    /// Cone vision (with direction and angle)
    Cone,
    /// 正方形视野
    /// Square vision
    Square,
}

impl Default for VisionSource {
    fn default() -> Self {
        Self {
            range: 100.0,
            enabled: true,
            shape: VisionShape::default(),
            direction: 0.0,
            angle: std::f32::consts::FRAC_PI_2, // 默认90度扇形 / Default 90 degree cone
            intensity: 1.0,
            transition_ratio: 0.2, // 默认20%的过渡区域 / Default 20% transition area
        }
    }
}

/// 区块的可见性状态
/// Visibility state of a chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Default)] // 允许通过反射获取默认值 / Allow getting default value via reflection
pub enum ChunkVisibility {
    /// 从未被任何视野源照亮过
    /// Never been revealed by any vision source
    #[default]
    Unexplored,
    /// 曾经被照亮过，但当前不在视野内
    /// Was revealed before, but not currently in vision
    Explored,
    /// 当前正被至少一个视野源照亮
    /// Currently being revealed by at least one vision source
    Visible,
}

impl Display for ChunkVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkVisibility::Unexplored => write!(f, "Unexplored"),
            ChunkVisibility::Explored => write!(f, "Explored"),
            ChunkVisibility::Visible => write!(f, "Visible"),
        }
    }
}

/// 地图区块组件，代表一个空间区域的迷雾和可见性数据
/// Fog chunk component, represents fog and visibility data for a spatial region
#[derive(Component, ExtractComponent, Reflect, Debug, Clone)]
pub struct FogChunk {
    /// 区块坐标
    /// Chunk coordinates
    pub coords: IVec2,
    /// 此区块在雾效 TextureArray 中的层索引
    /// Layer index for this chunk in the fog TextureArray
    pub fog_layer_index: Option<u32>,
    /// 此区块在快照 TextureArray 中的层索引
    /// Layer index for this chunk in the snapshot TextureArray
    pub snapshot_layer_index: Option<u32>,
    /// 区块的当前状态 (可见性与内存位置)
    /// Current state of the chunk (visibility and memory location)
    pub state: ChunkState,
    /// 区块的世界空间边界（以像素/单位为单位）
    /// World space boundaries of the chunk (in pixels/units)
    pub world_bounds: Rect,
}

impl FogChunk {
    pub fn unique_id(&self) -> u32 {
        let ox = (self.coords.x + 32768) as u32;
        let oy = (self.coords.y + 32768) as u32;
        (ox << 16) | (oy & 0xFFFF)
    }
    /// 创建一个新的地图区块
    /// Create a new map chunk
    pub fn new(chunk_coord: IVec2, size: UVec2, tile_size: f32) -> Self {
        let min = Vec2::new(
            chunk_coord.x as f32 * size.x as f32 * tile_size,
            chunk_coord.y as f32 * size.y as f32 * tile_size,
        );
        let max = min + Vec2::new(size.x as f32 * tile_size, size.y as f32 * tile_size);

        Self {
            coords: chunk_coord,
            fog_layer_index: None,
            snapshot_layer_index: None,
            state: Default::default(),
            world_bounds: Rect { min, max },
        }
    }

    /// 判断一个世界坐标是否在该区块内
    /// Check if a world coordinate is within this chunk
    pub fn contains_world_pos(&self, world_pos: Vec2) -> bool {
        self.world_bounds.contains(world_pos)
    }
}

#[derive(Component, Reflect, Debug, Clone)]
pub struct FogChunkImage {
    pub fog_image_handle: Handle<Image>,
    pub snapshot_image_handle: Handle<Image>,
}

impl FogChunkImage {
    pub fn from_setting(images: &mut ResMut<Assets<Image>>, setting: &FogMapSettings) -> Self {
        let data = vec![0u8; setting.fog_texture_format.pixel_size()];
        let mut fog_image = Image::new_fill(
            Extent3d {
                width: setting.texture_resolution_per_chunk.x,
                height: setting.texture_resolution_per_chunk.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &data,
            setting.fog_texture_format,
            RenderAssetUsages::default(),
        );
        fog_image.texture_descriptor.usage = TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
            | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输
        fog_image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
        let fog_image_handle = images.add(fog_image);

        let data = vec![0u8; setting.snapshot_texture_format.pixel_size()];
        let mut snapshot_image = Image::new_fill(
            Extent3d {
                width: setting.texture_resolution_per_chunk.x,
                height: setting.texture_resolution_per_chunk.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &data,
            setting.snapshot_texture_format,
            RenderAssetUsages::default(),
        );
        snapshot_image.texture_descriptor.usage = TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
            | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输
        snapshot_image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
        let snapshot_image_handle = images.add(snapshot_image);

        Self {
            fog_image_handle,
            snapshot_image_handle,
        }
    }
}

/// 区块纹理数据的存储位置
/// Storage location of the chunk's texture data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Default)]
pub enum ChunkMemoryLocation {
    /// 纹理数据存储在 GPU 显存中，可用于渲染
    /// Texture data resides in GPU VRAM, ready for rendering
    #[default]
    Gpu,
    /// 纹理数据已从 GPU 卸载，存储在 CPU 内存中
    /// Texture data is unloaded from GPU and stored in CPU RAM
    Cpu,
    /// 主世界已请求渲染世界将此区块数据从 GPU 复制到 CPU。等待 ChunkGpuDataReady。
    /// Main world has requested RenderWorld to copy this chunk's data from GPU. Awaiting ChunkGpuDataReady.
    PendingCopyToCpu,
    /// 主世界已请求渲染世界将 CPU 数据上传到此区块的 GPU 纹理。等待 ChunkCpuDataUploaded。
    /// Main world has requested RenderWorld to upload CPU data to this chunk's GPU texture. Awaiting ChunkCpuDataUploaded.
    PendingCopyToGpu,
}

/// 聚合区块状态
/// Aggregated chunk state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect, Default)]
#[reflect(Default)] // 允许通过反射获取默认值 / Allow getting default value via reflection
pub struct ChunkState {
    /// 可见性状态 / Visibility state
    pub visibility: ChunkVisibility,
    /// 内存存储位置 / Memory storage location
    pub memory_location: ChunkMemoryLocation,
}
