use crate::FogOfWarScreen;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use std::collections::HashMap;

pub const CHUNK_SIZE: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

impl ChunkCoord {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn from_world_pos(pos: Vec2) -> Self {
        Self {
            x: (pos.x as i32).div_euclid(CHUNK_SIZE as i32),
            y: (pos.y as i32).div_euclid(CHUNK_SIZE as i32),
        }
    }

    pub fn to_world_pos(&self) -> Vec2 {
        Vec2::new(
            (self.x * CHUNK_SIZE as i32) as f32,
            (self.y * CHUNK_SIZE as i32) as f32,
        )
    }
}

#[derive(Debug)]
pub struct Chunk {
    pub coord: ChunkCoord,
    pub is_loaded: bool,
}

impl Chunk {
    pub fn new(coord: ChunkCoord) -> Self {
        Self {
            coord,
            is_loaded: false,
        }
    }
}

#[derive(Component, Deref, DerefMut, Default)]
pub struct ChunkCache(pub Vec<u8>);

#[derive(Component, Debug)]
pub struct ChunkArrayIndex {
    pub index: i32,
}

pub fn update_chunks_system(
    fow_screen: Res<FogOfWarScreen>,
    mut commands: Commands,
    mut chunks_query: Query<(Entity, &ChunkCoord, &ChunkCache)>,
) {
    let chunks_in_view = fow_screen.get_chunks_in_view();
    let mut existing_coords: Vec<ChunkCoord> =
        chunks_query.iter().map(|(_, coord, _)| *coord).collect();

    // Handle chunk loading for new chunks
    for coord in chunks_in_view.iter() {
        if !existing_coords.contains(coord) {
            debug!("spawn coord: {:?} {:?}", coord, coord.to_world_pos());
            commands.spawn((*coord, ChunkCache::default(), ChunkArrayIndex { index: 0 }));
        }
    }
}

pub fn update_chunk_array_indices(
    fow_screen: Res<FogOfWarScreen>,
    mut query: Query<(&ChunkCoord, &mut ChunkArrayIndex)>,
) {
    // 计算视口可以容纳的块数（加上padding）
    let chunks_per_row = (fow_screen.screen_size.x / fow_screen.chunk_size).ceil() as i32 + 2;

    // 计算相机位置对应的chunk坐标
    let camera_chunk_x = (fow_screen.camera_position.x / fow_screen.chunk_size).floor() as i32;
    let camera_chunk_y = (fow_screen.camera_position.y / fow_screen.chunk_size).floor() as i32;

    // 修改为 -1 来保持对称的padding
    let top_left_chunk_x = camera_chunk_x - 1; // 1块padding
    let top_left_chunk_y = camera_chunk_y - 1;

    for (coord, mut array_index) in query.iter_mut() {
        // 将世界chunk坐标转换为相对于视口左上角的坐标
        let relative_x = coord.x - top_left_chunk_x;
        let relative_y = coord.y - top_left_chunk_y;

        // 计算数组索引
        array_index.index = relative_y * chunks_per_row + relative_x;
    }
}
