use crate::prelude::*;
use bevy::log::{debug, error, info, trace, warn};
use bevy::prelude::Resource;
use bevy::reflect::Reflect;
use std::collections::HashMap;
use std::collections::HashSet;

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

    /// 完全重置所有缓存，包括已探索区域，用于雾效重置。
    /// Completely reset all caches including explored areas, used for fog reset.
    pub fn reset_all(&mut self) {
        self.visible_chunks.clear();
        self.explored_chunks.clear();
        self.camera_view_chunks.clear();
        self.gpu_resident_chunks.clear();
    }
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
            trace!(
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
        for (coord, &indices) in &self.coord_to_layers {
            if indices == (fog_idx, snap_idx) {
                coord_to_remove = Some(*coord);
                break;
            }
        }
        if let Some(coord) = coord_to_remove {
            self.coord_to_layers.remove(&coord);
            debug!(
                "Removed coord {:?} for specific F{} S{}",
                coord, fog_idx, snap_idx
            );
        } else {
            warn!(
                "Attempted to free specific F{} S{} but no coord was using them.",
                fog_idx, snap_idx
            );
        }

        // It's crucial that an index is not pushed to free_..._indices
        // if it's already there or if it's invalid.
        // 关键是，如果索引已存在或无效，则不要将其推送到 free_..._indices。
        if !self.free_fog_indices.contains(&fog_idx) {
            // Basic check to prevent double free
            self.free_fog_indices.push(fog_idx);
        } else {
            warn!("Attempted to double-free specific fog index {}", fog_idx);
        }
        if !self.free_snapshot_indices.contains(&snap_idx) {
            self.free_snapshot_indices.push(snap_idx);
        } else {
            warn!(
                "Attempted to double-free specific snapshot index {}",
                snap_idx
            );
        }
    }

    pub fn get_allocated_indices(&self, coords: IVec2) -> Option<(u32, u32)> {
        self.coord_to_layers.get(&coords).copied()
    }

    pub fn is_coord_on_gpu(&self, coords: IVec2) -> bool {
        self.coord_to_layers.contains_key(&coords)
    }

    /// 清除所有分配的层索引，用于重置雾效系统。
    /// Clear all allocated layer indices, used for resetting the fog system.
    pub fn clear_all_layers(&mut self) {
        info!("Clearing all texture array layer allocations");

        // Clear the coord to layers mapping
        self.coord_to_layers.clear();

        // Reset all indices to free state
        self.free_fog_indices.clear();
        self.free_snapshot_indices.clear();

        for i in 0..self.capacity {
            self.free_fog_indices.push(i);
            self.free_snapshot_indices.push(i);
        }
    }

    /// 尝试为指定坐标分配特定的层索引（用于持久化恢复）
    /// Try to allocate specific layer indices for a coordinate (used for persistence restoration)
    pub fn allocate_specific_layer_indices(&mut self, coords: IVec2, fog_idx: u32, snap_idx: u32) -> bool {
        // Check if these indices are available
        if !self.free_fog_indices.contains(&fog_idx) || !self.free_snapshot_indices.contains(&snap_idx) {
            warn!("Cannot allocate specific indices F{} S{} for {:?} - indices not available", 
                  fog_idx, snap_idx, coords);
            return false;
        }

        // Check if coord already has layers
        if self.coord_to_layers.contains_key(&coords) {
            warn!("Cannot allocate specific indices for {:?} - coord already has layers", coords);
            return false;
        }

        // Remove indices from free lists
        self.free_fog_indices.retain(|&x| x != fog_idx);
        self.free_snapshot_indices.retain(|&x| x != snap_idx);

        // Add to mapping
        self.coord_to_layers.insert(coords, (fog_idx, snap_idx));

        debug!("Allocated specific layers for coord {:?}: F{} S{}", coords, fog_idx, snap_idx);
        true
    }

    /// 获取所有当前分配的层索引（用于持久化保存）
    /// Get all currently allocated layer indices (used for persistence saving)
    pub fn get_all_allocated_indices(&self) -> &HashMap<IVec2, (u32, u32)> {
        &self.coord_to_layers
    }
}
