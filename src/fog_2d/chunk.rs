use crate::FogOfWarScreen;
use bevy::prelude::*;
use std::collections::HashMap;

pub const CHUNK_SIZE: i32 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
            x: (pos.x as i32).div_euclid(CHUNK_SIZE),
            y: (pos.y as i32).div_euclid(CHUNK_SIZE),
        }
    }

    pub fn to_world_pos(&self) -> Vec2 {
        Vec2::new((self.x * CHUNK_SIZE) as f32, (self.y * CHUNK_SIZE) as f32)
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

#[derive(Resource)]
pub struct ChunkManager {
    chunks: HashMap<ChunkCoord, Chunk>,
    view_distance: i32,
}

impl Default for ChunkManager {
    fn default() -> Self {
        Self {
            chunks: HashMap::new(),
            view_distance: 2, // Default view distance in chunks
        }
    }
}

impl ChunkManager {
    pub fn new(view_distance: i32) -> Self {
        Self {
            chunks: HashMap::new(),
            view_distance,
        }
    }

    pub fn get_chunk(&self, coord: ChunkCoord) -> Option<&Chunk> {
        self.chunks.get(&coord)
    }

    pub fn get_chunk_mut(&mut self, coord: ChunkCoord) -> Option<&mut Chunk> {
        self.chunks.get_mut(&coord)
    }

    pub fn get_or_create_chunk(&mut self, coord: ChunkCoord) -> &mut Chunk {
        self.chunks
            .entry(coord)
            .or_insert_with(|| Chunk::new(coord))
    }

    pub fn get_chunks_in_view(&self, camera_pos: Vec2) -> Vec<ChunkCoord> {
        let center_chunk = ChunkCoord::from_world_pos(camera_pos);
        let mut chunks = Vec::new();

        for dx in -self.view_distance..=self.view_distance {
            for dy in -self.view_distance..=self.view_distance {
                chunks.push(ChunkCoord::new(center_chunk.x + dx, center_chunk.y + dy));
            }
        }

        chunks
    }

    pub fn update_chunks(&mut self, camera_pos: Vec2) -> (Vec<ChunkCoord>, Vec<ChunkCoord>) {
        let chunks_in_view = self.get_chunks_in_view(camera_pos);
        let mut to_load = Vec::new();
        let mut to_unload = Vec::new();

        // First create all chunks that should be in view
        for coord in &chunks_in_view {
            match self.chunks.entry(*coord) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    let chunk = entry.get_mut();
                    if !chunk.is_loaded {
                        to_load.push(*coord);
                    }
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(Chunk::new(*coord));
                    to_load.push(*coord);
                }
            }
        }

        // Then find chunks to unload (those that exist but are not in view)
        let chunks_to_unload: Vec<ChunkCoord> = self.chunks
            .iter()
            .filter(|(coord, chunk)| chunk.is_loaded && !chunks_in_view.contains(coord))
            .map(|(coord, _)| *coord)
            .collect();

        // Add to unload list and mark as unloaded
        for coord in chunks_to_unload {
            if let Some(chunk) = self.chunks.get_mut(&coord) {
                chunk.is_loaded = false;
                to_unload.push(coord);
            }
        }

        (to_load, to_unload)
    }
}

pub fn update_chunks_system(
    mut chunk_manager: ResMut<ChunkManager>,
    fow_screen: Res<FogOfWarScreen>,
) {
    let camera_pos = fow_screen.camera_position;
    let (to_load, to_unload) = chunk_manager.update_chunks(camera_pos);
    
    // Handle chunk loading
    for coord in to_load {
        if let Some(chunk) = chunk_manager.get_chunk_mut(coord) {
            chunk.is_loaded = true;
            debug!("Loading chunk at {:?}", coord);
        }
    }

    // Handle chunk unloading
    for coord in to_unload {
        if let Some(chunk) = chunk_manager.get_chunk_mut(coord) {
            chunk.is_loaded = false;
            debug!("Unloading chunk at {:?}", coord);
        }
    }
}
