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
    // Handle chunk loading
    for coord in fow_screen.get_chunks_in_view() {
        // Check if the coord is not already present in q_coords
        if q_coords.iter().all(|c| c != &coord) {
            debug!("spawn coord: {:?} {:?}", coord, coord.to_world_pos());
            commands.spawn(coord);
        }
    }
}
