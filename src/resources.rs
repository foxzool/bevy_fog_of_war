use crate::prelude::*;

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
        if let (Some(fog_idx), Some(snapshot_idx)) = (self.free_fog_indices.pop(), self.free_snapshot_indices.pop()) {
            // 检查索引是否在范围内 (虽然理论上 pop 出来的应该在) / Double check index bounds (though pop should guarantee it)
            if (fog_idx as usize) < self.fog_layers.len() && (snapshot_idx as usize) < self.snapshot_layers.len() {
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