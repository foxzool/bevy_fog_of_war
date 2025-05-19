use crate::prelude::*;
use bevy::color::palettes::basic;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::render::extract_resource::ExtractResource;
use bevy::render::render_resource::TextureFormat;

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

/// 存储可见性数据的 TextureArray 资源句柄
/// Resource handle for the TextureArray storing visibility data
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct VisibilityTextureArray {
    /// 图像资源的句柄 / Handle to the image asset
    pub handle: Handle<Image>,
}

/// 存储雾效数据的 TextureArray 资源句柄
/// Resource handle for the TextureArray storing fog data
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct FogTextureArray {
    /// 图像资源的句柄 / Handle to the image asset
    pub handle: Handle<Image>,
}

/// 存储快照数据的 TextureArray 资源句柄
/// Resource handle for the TextureArray storing snapshot data
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct SnapshotTextureArray {
    /// 图像资源的句柄 / Handle to the image asset
    pub handle: Handle<Image>,
}

#[derive(Resource, Debug, Reflect)]
#[reflect(Resource)]
pub struct TextureArrayManager {
    capacity: u32,
    // Maps chunk coordinates to the layer indices they currently occupy on the GPU
    // 将区块坐标映射到它们当前在 GPU 上占用的层索引
    coord_to_layers: HashMap<IVec2, (u32, u32)>, // (fog_idx, snapshot_idx)
    // Stores layer indices that are currently free to be allocated
    // 存储当前可以自由分配的层索引
    // Using Vec as a simple stack for free indices
    // 使用 Vec 作为空闲索引的简单堆栈
    free_fog_indices: Vec<u32>,
    free_snapshot_indices: Vec<u32>,
    // Or, if fog and snapshot always use paired indices (e.g., fog layer X always pairs with snapshot layer X)
    // 或者，如果雾效和快照始终使用配对索引 (例如，雾效层 X 始终与快照层 X 配对)
    // free_paired_indices: Vec<u32>,
}

impl TextureArrayManager {
    pub fn new(array_layers_capacity: u32) -> Self {
        // Initialize all layers as free
        // 将所有层初始化为空闲
        let mut free_fog = Vec::with_capacity(array_layers_capacity as usize);
        let mut free_snap = Vec::with_capacity(array_layers_capacity as usize);
        for i in 0..array_layers_capacity {
            free_fog.push(i);
            free_snap.push(i); // Assuming separate pools for simplicity, or they could be linked
        }
        Self {
            capacity: array_layers_capacity,
            coord_to_layers: HashMap::new(),
            free_fog_indices: free_fog,
            free_snapshot_indices: free_snap,
        }
    }

    /// Allocates a pair of layer indices for a given chunk coordinate.
    /// Returns None if no free layers are available.
    /// 为给定的区块坐标分配一对层索引。
    /// 如果没有可用的空闲层，则返回 None。
    pub fn allocate_layer_indices(&mut self, coords: IVec2) -> Option<(u32, u32)> {
        if self.coord_to_layers.contains_key(&coords) {
            // This coord already has layers, should not happen if logic is correct.
            // Or, it means we are re-activating a chunk that somehow wasn't fully cleaned up.
            // 这个坐标已经有层了，如果逻辑正确则不应发生。
            // 或者，这意味着我们正在重新激活一个不知何故未完全清理的区块。
            warn!(
                "Attempted to allocate layers for {:?} which already has layers: {:?}. Reusing.",
                coords,
                self.coord_to_layers.get(&coords)
            );
            return self.coord_to_layers.get(&coords).copied();
        }

        if let (Some(fog_idx), Some(snap_idx)) = (
            self.free_fog_indices.pop(),
            self.free_snapshot_indices.pop(),
        ) {
            self.coord_to_layers.insert(coords, (fog_idx, snap_idx));
            debug!(
                "Allocating layers for coord {:?}. F{} S{}",
                coords, fog_idx, snap_idx
            );
            Some((fog_idx, snap_idx))
        } else {
            // Ran out of layers, push back any popped indices if one succeeded but other failed (shouldn't happen with paired pop)
            // 层用完了，如果一个成功但另一个失败，则推回任何弹出的索引 (配对弹出不应发生这种情况)
            // This logic needs to be robust if fog/snapshot indices are truly independent.
            // 如果雾效/快照索引真正独立，则此逻辑需要稳健。
            // For now, assuming paired allocation success/failure.
            // 目前假设配对分配成功/失败。
            error!("TextureArrayManager: No free layers available!");
            None
        }
    }

    /// Frees the layer indices associated with a given chunk coordinate.
    /// 释放与给定区块坐标关联的层索引。
    pub fn free_layer_indices_for_coord(&mut self, coords: IVec2) {
        if let Some((fog_idx, snap_idx)) = self.coord_to_layers.remove(&coords) {
            info!(
                "Freeing layers for coord {:?}. F{} S{}",
                coords, fog_idx, snap_idx
            );
            // It's crucial that an index is not pushed to free_..._indices
            // if it's already there or if it's invalid.
            // 关键是，如果索引已存在或无效，则不要将其推送到 free_..._indices。
            if !self.free_fog_indices.contains(&fog_idx) {
                // Basic check to prevent double free
                self.free_fog_indices.push(fog_idx);
            } else {
                warn!(
                    "Attempted to double-free fog index {} for coord {:?}",
                    fog_idx, coords
                );
            }
            if !self.free_snapshot_indices.contains(&snap_idx) {
                self.free_snapshot_indices.push(snap_idx);
            } else {
                warn!(
                    "Attempted to double-free snapshot index {} for coord {:?}",
                    snap_idx, coords
                );
            }
        } else {
            warn!(
                "Attempted to free layers for coord {:?} which has no allocated layers.",
                coords
            );
        }
    }

    /// Frees specific layer indices. This is used when FogChunk directly provides indices.
    /// 释放特定的层索引。当 FogChunk 直接提供索引时使用。
    pub fn free_specific_layer_indices(&mut self, fog_idx: u32, snap_idx: u32) {
        info!("Freeing specific layer indices {} {}", fog_idx, snap_idx);
        // We also need to find which coord was using these indices to remove it from coord_to_layers
        // 我们还需要找出哪个坐标正在使用这些索引，以便从 coord_to_layers 中删除它
        let mut coord_to_remove = None;
        for (coord, (f_idx, s_idx)) in &self.coord_to_layers {
            if *f_idx == fog_idx && *s_idx == snap_idx {
                coord_to_remove = Some(*coord);
                break;
            }
        }
        if let Some(coord) = coord_to_remove {
            self.coord_to_layers.remove(&coord);
        } else {
            // warn!("Tried to free specific indices ({}, {}) that were not mapped to any coord.", fog_idx, snap_idx);
        }

        if !self.free_fog_indices.contains(&fog_idx) {
            self.free_fog_indices.push(fog_idx);
        } else {
            // warn!("Attempted to double-free specific fog index {}", fog_idx);
        }
        if !self.free_snapshot_indices.contains(&snap_idx) {
            self.free_snapshot_indices.push(snap_idx);
        } else {
            // warn!("Attempted to double-free specific snapshot index {}", snap_idx);
        }
    }

    pub fn get_allocated_indices(&self, coords: IVec2) -> Option<(u32, u32)> {
        self.coord_to_layers.get(&coords).copied()
    }

    pub fn is_coord_on_gpu(&self, coords: IVec2) -> bool {
        self.coord_to_layers.contains_key(&coords)
    }
}

/// 战争迷雾地图的全局设置
/// Global settings for the fog of war map
#[derive(Resource, ExtractResource, Clone, Debug)]
pub struct FogMapSettings {
    /// 是否启用战争迷雾系统
    /// Whether the fog of war system is enabled
    pub enabled: bool,
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
    /// 最大允许的区块数量
    /// Maximum number of allowed chunks
    pub max_layers: u32,
}

impl Default for FogMapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            chunk_size: UVec2::splat(256),
            texture_resolution_per_chunk: UVec2::new(512, 512), // 示例分辨率 / Example resolution
            fog_color_unexplored: Color::BLACK,
            fog_color_explored: basic::GRAY.into(),
            vision_clear_color: Color::NONE,
            // R8Unorm 通常足够表示雾的浓度 (0.0 可见, 1.0 遮蔽)
            // R8Unorm is often sufficient for fog density (0.0 visible, 1.0 obscured)
            fog_texture_format: TextureFormat::R8Unorm,
            // 快照需要颜色和透明度 / Snapshots need color and alpha
            snapshot_texture_format: TextureFormat::Rgba8UnormSrgb,
            max_layers: 64,
        }
    }
}

impl FogMapSettings {
    pub fn chunk_coord_to_world(&self, chunk_coord: IVec2) -> Vec2 {
        Vec2::new(
            chunk_coord.x as f32 * self.chunk_size.x as f32,
            chunk_coord.y as f32 * self.chunk_size.y as f32,
        )
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
    pub fog_image_handle: Handle<Image>,      
    pub snapshot_image_handle: Handle<Image>,
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
