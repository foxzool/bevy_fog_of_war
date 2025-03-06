use bevy::prelude::*;

/// Chunk组件，表示游戏世界中的一个区域
/// Chunk component, represents an area in the game world
#[derive(Debug, Clone, Copy, Component)]
pub struct Chunk {
    /// 区块的索引坐标
    /// Index coordinates of the chunk
    pub index: IVec2,
    /// 区块大小（以世界单位表示）
    /// Size of the chunk (in world units)
    pub size: f32,
}

// 手动实现PartialEq，只比较索引，不比较大小
// Manually implement PartialEq, only compare indices, not size
impl PartialEq for Chunk {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

// 手动实现Eq特性
// Manually implement Eq trait
impl Eq for Chunk {}

// 手动实现Hash特性，只对索引进行哈希
// Manually implement Hash trait, only hash the index
impl std::hash::Hash for Chunk {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl Chunk {
    /// 创建一个新的区块
    /// Create a new chunk
    pub fn new(index: IVec2, size: f32) -> Self {
        Self { index, size }
    }

    /// 从世界坐标创建区块
    /// Create a chunk from world coordinates
    pub fn from_world_pos(world_pos: Vec2, chunk_size: f32) -> Self {
        let index = Self::world_pos_to_chunk_index(world_pos, chunk_size);
        Self::new(index, chunk_size)
    }

    /// 将世界坐标转换为区块索引
    /// Convert world coordinates to chunk index
    pub fn world_pos_to_chunk_index(world_pos: Vec2, chunk_size: f32) -> IVec2 {
        IVec2::new(
            (world_pos.x / chunk_size).floor() as i32,
            (world_pos.y / chunk_size).floor() as i32,
        )
    }

    /// 获取区块的世界坐标位置(左下角)
    /// Get the world coordinate position of the chunk (bottom-left corner)
    pub fn get_world_pos(&self) -> Vec2 {
        Vec2::new(
            self.index.x as f32 * self.size,
            self.index.y as f32 * self.size,
        )
    }

    /// 获取区块的中心点世界坐标
    /// Get the world coordinates of the center point of the chunk
    pub fn get_center_world_pos(&self) -> Vec2 {
        self.get_world_pos() + Vec2::new(self.size / 2.0, self.size / 2.0)
    }

    /// 判断世界坐标是否在此区块内
    /// Determine if world coordinates are within this chunk
    pub fn contains(&self, world_pos: Vec2) -> bool {
        let chunk_pos = self.get_world_pos();
        world_pos.x >= chunk_pos.x
            && world_pos.x < chunk_pos.x + self.size
            && world_pos.y >= chunk_pos.y
            && world_pos.y < chunk_pos.y + self.size
    }
}

/// 区块管理资源，用于管理区块的生成和销毁
/// Chunk manager resource for managing chunk creation and destruction
#[derive(Resource)]
pub struct ChunkManager {
    /// 区块大小常量(世界单位)
    /// Chunk size constant (world units)
    pub chunk_size: f32,
    /// 视野范围（以区块为单位）
    /// View range (in chunks)
    pub view_range: u32,
}

impl Default for ChunkManager {
    fn default() -> Self {
        Self {
            chunk_size: 256.0,
            view_range: 3,
        }
    }
}

impl ChunkManager {
    /// 创建一个新的区块管理器
    /// Create a new chunk manager
    pub fn new(chunk_size: f32, view_range: u32) -> Self {
        Self {
            chunk_size,
            view_range,
        }
    }

    /// 从世界坐标获取对应的区块索引
    /// Get the corresponding chunk index from world coordinates
    pub fn get_chunk_index(&self, world_pos: Vec2) -> IVec2 {
        Chunk::world_pos_to_chunk_index(world_pos, self.chunk_size)
    }

    /// 获取相机视野内的所有区块索引
    /// Get all chunk indices within the camera's view
    pub fn get_chunks_in_camera_view(&self, camera_position: Vec2) -> Vec<IVec2> {
        let center_chunk_index = self.get_chunk_index(camera_position);
        let mut chunk_indices = Vec::new();

        let range = self.view_range as i32;
        for y in -range..=range {
            for x in -range..=range {
                chunk_indices.push(center_chunk_index + IVec2::new(x, y));
            }
        }

        chunk_indices
    }
}

/// 标记需要生成的区块
/// Marker component for chunks that need to be generated
#[derive(Component)]
pub struct ChunkNeedsGeneration;
