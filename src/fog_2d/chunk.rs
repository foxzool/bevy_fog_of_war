use crate::FogOfWarScreen;
use bevy::prelude::*;
use std::collections::HashMap;

pub const CHUNK_SIZE: i32 = 512;

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

    pub fn get_chunks_in_view(&self, camera_pos: Vec2, window_size: Vec2) -> Vec<ChunkCoord> {
        let mut chunks = Vec::new();

        // Calculate the bounds of the visible area in world coordinates
        let half_width = window_size.x / 2.0;
        let half_height = window_size.y / 2.0;
        let min_x = camera_pos.x - half_width;
        let max_x = camera_pos.x + half_width;
        let min_y = camera_pos.y - half_height;
        let max_y = camera_pos.y + half_height;

        // Convert to chunk coordinates and add 1 to ensure coverage
        let start_chunk_x = (min_x as i32).div_euclid(CHUNK_SIZE) - 1;
        let end_chunk_x = (max_x as i32).div_euclid(CHUNK_SIZE) + 1;
        let start_chunk_y = (min_y as i32).div_euclid(CHUNK_SIZE) - 1;
        let end_chunk_y = (max_y as i32).div_euclid(CHUNK_SIZE) + 1;

        // Collect all chunks that intersect with the visible area
        for x in start_chunk_x..=end_chunk_x {
            for y in start_chunk_y..=end_chunk_y {
                chunks.push(ChunkCoord::new(x, y));
            }
        }

        chunks
    }

    pub fn update_chunks(
        &mut self,
        camera_pos: Vec2,
        window_size: Vec2,
    ) -> (Vec<ChunkCoord>, Vec<ChunkCoord>) {
        let chunks_in_view = self.get_chunks_in_view(camera_pos, window_size);
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
        let chunks_to_unload: Vec<ChunkCoord> = self
            .chunks
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
    mut commands: Commands,
    mut chunk_manager: ResMut<ChunkManager>,
    fow_screen: Res<FogOfWarScreen>,
    q_coords: Query<&ChunkCoord>,
) {
    let chunks_in_view = fow_screen.get_chunks_in_view();

    // Handle chunk loading
    for coord in chunks_in_view {
        // Check if the coord is not already present in q_coords
        if q_coords.iter().all(|c| c != &coord) {
            debug!("spawn coord: {:?} {:?}", coord, coord.to_world_pos());
            commands.spawn(coord);
        }
    }
}
