use crate::FogOfWarScreen;
use bevy::prelude::*;
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

pub fn update_chunks_system(
    mut commands: Commands,
    fow_screen: Res<FogOfWarScreen>,
    q_coords: Query<&ChunkCoord>,
) {
    let half_width = fow_screen.window_size.x * 0.5;
    let half_height = fow_screen.window_size.y * 0.5;
    let min_x = fow_screen.camera_position.x - half_width;
    let max_x = fow_screen.camera_position.x + half_width;
    let min_y = fow_screen.camera_position.y - half_height;
    let max_y = fow_screen.camera_position.y + half_height;

    // Convert to chunk coordinates and add 1 to ensure coverage
    let start_chunk_x = (min_x as i32).div_euclid(CHUNK_SIZE as i32) - 1;
    let end_chunk_x = (max_x as i32).div_euclid(CHUNK_SIZE as i32) + 1;
    let start_chunk_y = (min_y as i32).div_euclid(CHUNK_SIZE as i32) - 1;
    let end_chunk_y = (max_y as i32).div_euclid(CHUNK_SIZE as i32) + 1;

    // Collect all chunks that intersect with the visible area
    let mut chunks_in_view = Vec::new();
    for x in start_chunk_x..=end_chunk_x {
        for y in start_chunk_y..=end_chunk_y {
            chunks_in_view.push(ChunkCoord::new(x, y));
        }
    }

    // Handle chunk loading
    for coord in chunks_in_view {
        // Check if the coord is not already present in q_coords
        if q_coords.iter().all(|c| c != &coord) {
            debug!("spawn coord: {:?} {:?}", coord, coord.to_world_pos());
            commands.spawn(coord);
        }
    }
}
