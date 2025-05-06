use crate::prelude::*;
use bevy::color::palettes::basic;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::render::extract_resource::ExtractResource;
use bevy::render::render_resource::TextureFormat;
use std::sync::Arc;

/// 快速查找区块坐标对应的 FogChunk 实体
/// Resource for quickly looking up FogChunk entities by their coordinates
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource, Default)] // 注册为反射资源, 并提供默认值反射 / Register as reflectable resource with default reflection
pub struct ChunkEntityManager {
    /// 从区块坐标到实体 ID 的映射
    /// Map from chunk coordinates to Entity ID
    pub map: HashMap<IVec2, Entity>,
}

/// 缓存各种状态的区块坐标集合，用于系统间的快速查询
/// Resource caching sets of chunk coordinates in various states for fast querying between systems
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource, Default)] // 注册为反射资源, 并提供默认值反射 / Register as reflectable resource with default reflection
pub struct ChunkStateCache {
    /// 当前被至少一个 VisionSource 照亮的区块坐标集合
    /// Set of chunk coordinates currently revealed by at least one VisionSource
    pub visible_chunks: HashSet<IVec2>,
    /// 曾经被照亮过的区块坐标集合 (包含 visible_chunks)
    /// Set of chunk coordinates that have ever been revealed (includes visible_chunks)
    pub explored_chunks: HashSet<IVec2>,
    /// 当前在主相机视锥范围内的区块坐标集合
    /// Set of chunk coordinates currently within the main camera's view frustum
    pub camera_view_chunks: HashSet<IVec2>,
    /// 其纹理当前存储在 GPU 显存中的区块坐标集合
    /// Set of chunk coordinates whose textures are currently resident in GPU memory
    pub gpu_resident_chunks: HashSet<IVec2>,
}

impl ChunkStateCache {
    /// 清除所有缓存的区块集合，通常在每帧开始时调用
    /// Clears all cached chunk sets, typically called at the beginning of each frame
    pub fn clear(&mut self) {
        self.visible_chunks.clear();
        // explored_chunks 通常不清空，除非需要重置迷雾 / explored_chunks is usually not cleared unless resetting fog
        self.camera_view_chunks.clear();
        // gpu_resident_chunks 的管理更复杂，不一定每帧清空 / gpu_resident_chunks management is more complex, not necessarily cleared every frame
    }
}

/// 存储雾效数据的 TextureArray 资源句柄
/// Resource handle for the TextureArray storing fog data
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct FogTextureArray {
    /// 图像资源的句柄 / Handle to the image asset
    pub handle: Handle<Image>,
}
// FogTextureArray 通常在 setup 系统中创建并插入，没有 Default
// FogTextureArray is usually created and inserted in a setup system, no Default

/// 存储快照数据的 TextureArray 资源句柄
/// Resource handle for the TextureArray storing snapshot data
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct SnapshotTextureArray {
    /// 图像资源的句柄 / Handle to the image asset
    pub handle: Handle<Image>,
}
// SnapshotTextureArray 通常在 setup 系统中创建并插入，没有 Default
// SnapshotTextureArray is usually created and inserted in a setup system, no Default

/// 管理 TextureArray 中层的使用情况
/// Manages the usage of layers within the TextureArrays
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource, Default)] // 注册为反射资源, 并提供默认值反射 / Register as reflectable resource with default reflection
pub struct TextureArrayManager {
    /// 记录雾效 TextureArray 每一层被哪个区块坐标使用 (None 表示空闲)
    /// Records which chunk coordinates use each layer of the fog TextureArray (None means free)
    pub fog_layers: Vec<Option<IVec2>>,
    /// 记录快照 TextureArray 每一层被哪个区块坐标使用 (None 表示空闲)
    /// Records which chunk coordinates use each layer of the snapshot TextureArray (None means free)
    pub snapshot_layers: Vec<Option<IVec2>>,
    /// 空闲的雾效层索引列表
    /// List of free fog layer indices
    pub free_fog_indices: Vec<u32>,
    /// 空闲的快照层索引列表
    /// List of free snapshot layer indices
    pub free_snapshot_indices: Vec<u32>,
    // 可以添加 capacity 字段来表示数组的总层数
    // A capacity field could be added to represent the total number of layers in the arrays
    // pub capacity: u32,
}

impl TextureArrayManager {
    /// 初始化管理器，指定 TextureArray 的总层数
    /// Initializes the manager, specifying the total number of layers in the TextureArrays
    pub fn new(capacity: u32) -> Self {
        let capacity_usize = capacity as usize;
        Self {
            fog_layers: vec![None; capacity_usize],
            snapshot_layers: vec![None; capacity_usize],
            // 初始时所有索引都是空闲的，倒序填充方便 pop / Initially all indices are free, fill in reverse for easy pop
            free_fog_indices: (0..capacity).rev().collect(),
            free_snapshot_indices: (0..capacity).rev().collect(),
            // capacity: capacity,
        }
    }

    /// 分配一个空闲的层索引对 (雾效, 快照)
    /// Allocates a pair of free layer indices (fog, snapshot)
    pub fn allocate_layer_indices(&mut self, coords: IVec2) -> Option<(u32, u32)> {
        if let (Some(fog_idx), Some(snapshot_idx)) = (
            self.free_fog_indices.pop(),
            self.free_snapshot_indices.pop(),
        ) {
            // 检查索引是否在范围内 (虽然理论上 pop 出来的应该在) / Double check index bounds (though pop should guarantee it)
            if (fog_idx as usize) < self.fog_layers.len()
                && (snapshot_idx as usize) < self.snapshot_layers.len()
            {
                self.fog_layers[fog_idx as usize] = Some(coords);
                self.snapshot_layers[snapshot_idx as usize] = Some(coords);
                Some((fog_idx, snapshot_idx))
            } else {
                // 如果索引无效，放回去 / If index is invalid, put them back
                self.free_fog_indices.push(fog_idx);
                self.free_snapshot_indices.push(snapshot_idx);
                None // 理论上不应发生 / Should not happen theoretically
            }
        } else {
            // 没有足够的空闲索引 / Not enough free indices
            None
        }
    }

    /// 释放指定索引对，使其可被重用
    /// Frees the specified index pair, making them available for reuse
    pub fn free_layer_indices(&mut self, fog_idx: u32, snapshot_idx: u32) {
        if (fog_idx as usize) < self.fog_layers.len() {
            self.fog_layers[fog_idx as usize] = None;
            self.free_fog_indices.push(fog_idx); // 可以考虑排序或保持无序 / Can consider sorting or keeping unsorted
        }
        if (snapshot_idx as usize) < self.snapshot_layers.len() {
            self.snapshot_layers[snapshot_idx as usize] = None;
            self.free_snapshot_indices.push(snapshot_idx);
        }
    }

    // 可以添加更多辅助方法，例如根据坐标查找索引等
    // More helper methods can be added, e.g., finding indices by coords, etc.
}

/// 战争迷雾地图的全局设置
/// Global settings for the fog of war map
#[derive(Resource, ExtractResource, Clone, Debug)]
pub struct FogMapSettings {
    /// 每个区块的大小 (世界单位)
    /// Size of each chunk (in world units)
    pub chunk_size: UVec2,
    /// 每个区块对应纹理的分辨率 (像素)
    /// Resolution of the texture per chunk (in pixels)
    pub texture_resolution_per_chunk: UVec2,
    /// 未探索区域的雾颜色
    /// Fog color for unexplored areas
    pub fog_color_unexplored: Color,
    /// 已探索但当前不可见区域的雾颜色 (通常是半透明)
    /// Fog color for explored but not currently visible areas (usually semi-transparent)
    pub fog_color_explored: Color,
    /// 视野完全清晰区域的“颜色”（通常用于混合或阈值，可能完全透明）
    /// "Color" for fully visible areas (often used for blending or thresholds, might be fully transparent)
    pub vision_clear_color: Color, // 例如 Color::NONE 或用于计算的特定值 / e.g., Color::NONE or a specific value for calculations
    /// 雾效纹理数组的格式
    /// Texture format for the fog texture array ]
    pub fog_texture_format: TextureFormat,
    /// 快照纹理数组的格式
    /// Texture format for the snapshot texture array
    pub snapshot_texture_format: TextureFormat,
}

impl Default for FogMapSettings {
    fn default() -> Self {
        Self {
            chunk_size: UVec2::splat(256),
            texture_resolution_per_chunk: UVec2::new(128, 128), // 示例分辨率 / Example resolution
            fog_color_unexplored: Color::BLACK,
            fog_color_explored: basic::GRAY.into(),
            vision_clear_color: Color::NONE,
            // R8Unorm 通常足够表示雾的浓度 (0.0 可见, 1.0 遮蔽)
            // R8Unorm is often sufficient for fog density (0.0 visible, 1.0 obscured)
            fog_texture_format: TextureFormat::R8Unorm,
            // 快照需要颜色和透明度 / Snapshots need color and alpha
            snapshot_texture_format: TextureFormat::Rgba8UnormSrgb,
        }
    }
}

/// Information for a single snapshot request, generated in the main world.
/// 单个快照请求的信息，在主世界中生成。
#[derive(Debug, Clone, Reflect)]
pub struct MainWorldSnapshotRequest {
    pub chunk_coords: IVec2,
    pub snapshot_layer_index: u32,
    pub world_bounds: Rect,
}

/// Resource in the main world to queue chunks that need a snapshot.
/// 主世界中的资源，用于对需要快照的区块进行排队。
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource, Default)]
pub struct MainWorldSnapshotRequestQueue {
    pub requests: Vec<MainWorldSnapshotRequest>,
}

/// 由主世界填充，请求渲染世界将 GPU 纹理数据复制到 CPU。
/// Populated by the main world to request the render world to copy GPU texture data to CPU.
#[derive(Resource, Default, Debug, Clone, Reflect, ExtractResource)]
#[reflect(Resource, Default)]
pub struct GpuToCpuCopyRequests {
    pub requests: Vec<GpuToCpuCopyRequest>,
}

#[derive(Debug, Clone, Reflect)]
pub struct GpuToCpuCopyRequest {
    pub chunk_coords: IVec2,
    pub fog_layer_index: u32,
    pub snapshot_layer_index: u32,
    // Staging buffer index or some identifier if RenderApp uses a pool
    // 如果 RenderApp 使用池，则为暂存缓冲区索引或某种标识符
}
/// 由主世界填充，请求渲染世界将 CPU 纹理数据上传到 GPU。
/// Populated by the main world to request the render world to upload CPU texture data to GPU.
#[derive(Resource, Default, Debug, Clone, Reflect, ExtractResource)]
#[reflect(Resource, Default)]
pub struct CpuToGpuCopyRequests {
    pub requests: Vec<CpuToGpuCopyRequest>,
}

#[derive(Debug, Clone, Reflect)]
pub struct CpuToGpuCopyRequest {
    pub chunk_coords: IVec2,
    pub fog_layer_index: u32,
    pub snapshot_layer_index: u32,
    pub fog_data: Vec<u8>,      // Raw texture data for fog / 雾效的原始纹理数据
    pub snapshot_data: Vec<u8>, // Raw texture data for snapshot / 快照的原始纹理数据
}

/// 事件：当 GPU 数据成功复制到 CPU 并可供主世界使用时，由 RenderApp 发送。
/// Event: Sent by RenderApp when GPU data has been successfully copied to CPU and is available to the main world.
#[derive(Event, Debug)]
pub struct ChunkGpuDataReadyEvent {
    pub chunk_coords: IVec2,
    pub fog_data: Vec<u8>,
    pub snapshot_data: Vec<u8>,
}

/// 事件：当 CPU 数据成功上传到 GPU 时，由 RenderApp 发送。
/// Event: Sent by RenderApp when CPU data has been successfully uploaded to GPU.
#[derive(Event, Debug)]
pub struct ChunkCpuDataUploadedEvent {
    pub chunk_coords: IVec2,
}
