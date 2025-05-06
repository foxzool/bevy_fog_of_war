use crate::prelude::*;
use bevy::platform::collections::HashMap;
use bevy::render::camera::RenderTarget;
use bevy::render::extract_component::ExtractComponent;

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
}

impl Default for VisionSource {
    fn default() -> Self {
        Self {
            range: 100.0,
            enabled: true,
        }
    }
}

/// 区块的可见性状态
/// Visibility state of a chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Default)] // 允许通过反射获取默认值 / Allow getting default value via reflection
pub enum ChunkVisibility {
    /// 从未被任何视野源照亮过
    /// Never been revealed by any vision source
    Unexplored,
    /// 曾经被照亮过，但当前不在视野内
    /// Was revealed before, but not currently in vision
    Explored,
    /// 当前正被至少一个视野源照亮
    /// Currently being revealed by at least one vision source
    Visible,
}

impl Default for ChunkVisibility {
    fn default() -> Self {
        ChunkVisibility::Unexplored
    }
}

/// 地图区块组件，代表一个空间区域的迷雾和可见性数据
/// Fog chunk component, represents fog and visibility data for a spatial region
#[derive(Component, ExtractComponent, Reflect, Debug, Clone)]
pub struct FogChunk {
    /// 区块坐标
    /// Chunk coordinates
    pub coords: IVec2,
    pub layer_index: Option<u32>,
    pub screen_index: Option<u32>,
    /// 此区块在雾效 TextureArray 中的层索引
    /// Layer index for this chunk in the fog TextureArray
    pub fog_layer_index: u32,
    /// 此区块在快照 TextureArray 中的层索引
    /// Layer index for this chunk in the snapshot TextureArray
    pub snapshot_layer_index: u32,
    /// 是否加载
    /// Whether the chunk is loaded
    pub loaded: bool,
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
    pub fn new(chunk_coord: ChunkCoord, size: UVec2, tile_size: f32) -> Self {
        let min = Vec2::new(
            chunk_coord.x as f32 * size.x as f32 * tile_size,
            chunk_coord.y as f32 * size.y as f32 * tile_size,
        );
        let max = min + Vec2::new(size.x as f32 * tile_size, size.y as f32 * tile_size);

        Self {
            coords: chunk_coord,
            layer_index: None,
            screen_index: None,
            fog_layer_index: 0,
            snapshot_layer_index: 0,
            loaded: true,
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

// 区块纹理数据的存储位置
/// Storage location of the chunk's texture data
/// 区块纹理数据的存储位置
/// Storage location of the chunk's texture data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Default)]
pub enum ChunkMemoryLocation {
    /// 纹理数据存储在 GPU 显存中，可用于渲染
    /// Texture data resides in GPU VRAM, ready for rendering
    Gpu,
    /// 纹理数据已从 GPU 卸载，存储在 CPU 内存中
    /// Texture data is unloaded from GPU and stored in CPU RAM
    Cpu,
    /// 主世界已请求渲染世界将此区块数据从 GPU 复制到 CPU。等待 ChunkGpuDataReadyEvent。
    /// Main world has requested RenderWorld to copy this chunk's data from GPU. Awaiting ChunkGpuDataReadyEvent.
    PendingCopyToCpu,
    /// 主世界已请求渲染世界将 CPU 数据上传到此区块的 GPU 纹理。等待 ChunkCpuDataUploadedEvent。
    /// Main world has requested RenderWorld to upload CPU data to this chunk's GPU texture. Awaiting ChunkCpuDataUploadedEvent.
    PendingCopyToGpu,
}

impl Default for ChunkMemoryLocation {
    fn default() -> Self {
        ChunkMemoryLocation::Gpu // Or Cpu, depending on initial creation strategy
        // 或 Cpu, 取决于初始创建策略
    }
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

/// 在 CPU 内存中存储已卸载的区块纹理数据
/// Resource for storing unloaded chunk texture data in CPU memory
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource, Default)] // 注册为反射资源, 并提供默认值反射 / Register as reflectable resource with default reflection
pub struct CpuChunkStorage {
    /// 从区块坐标到 (雾效原始数据, 快照原始数据) 的映射
    /// Map from chunk coordinates to (raw fog data, raw snapshot data)
    /// Vec<u8> 存储了对应纹理格式的字节数据
    /// Vec<u8> stores the byte data for the corresponding texture format
    pub storage: HashMap<IVec2, (Vec<u8>, Vec<u8>)>,
}

/// 标记组件，指示该实体应被包含在战争迷雾的快照中
/// Marker component indicating this entity should be included in the fog of war snapshot
#[derive(Component, Debug, Clone, Default, Reflect)]
#[reflect(Component, Default)]
pub struct Snapshottable {
    pub priority: u8,
}

/// Marker component for a camera used to render snapshots.
/// 用于渲染快照的相机的标记组件。
#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct SnapshotCamera;

/// Stores the current target for a snapshot camera during rendering.
/// Not an ExtractComponent, managed internally in RenderApp.
/// 在渲染期间存储快照相机的当前目标。
/// 不是 ExtractComponent，在 RenderApp 内部管理。
#[derive(Component)]
pub struct SnapshotCameraTarget {
    pub render_target: RenderTarget,
    pub world_bounds: Rect, // To help with culling or setting projection
                            // 帮助剔除或设置投影
}
