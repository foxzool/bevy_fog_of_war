use crate::FogOfWarScreen;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
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

#[derive(Component)]
pub struct ChunkImage {
    pub image: Handle<Image>,
}

#[derive(Component, Debug)]
pub struct ChunkArrayIndex {
    pub index: i32,
}

pub fn update_chunks_system(
    fow_screen: Res<FogOfWarScreen>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut chunks_query: Query<(Entity, &ChunkCoord, &ChunkImage, Option<&Readback>)>,
) {
    let chunks_in_view = fow_screen.get_chunks_in_view();
    let mut existing_coords: Vec<ChunkCoord> =
        chunks_query.iter().map(|(_, coord, _, _)| *coord).collect();

    // Handle chunk loading for new chunks
    for coord in chunks_in_view.iter() {
        if !existing_coords.contains(coord) {
            let mut image = Image::new_fill(
                Extent3d {
                    width: fow_screen.chunk_size as u32,
                    height: fow_screen.chunk_size as u32,
                    ..default()
                },
                TextureDimension::D2,
                &[0, 0, 0, 0],
                TextureFormat::R32Uint,
                RenderAssetUsages::RENDER_WORLD,
            );
            image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
            let image = images.add(image);
            debug!("spawn coord: {:?} {:?}", coord, coord.to_world_pos());
            commands.spawn((
                *coord,
                Readback::texture(image.clone()),
                ChunkImage { image },
                ChunkArrayIndex { index: 0 },
            ));
        }
    }

    // Update Readback components based on visibility
    for (entity, coord, chunk_image, readback) in chunks_query.iter() {
        let is_in_view = chunks_in_view.contains(coord);
        match (is_in_view, readback) {
            (true, None) => {
                // Add Readback if chunk is in view but doesn't have it
                commands
                    .entity(entity)
                    .insert(Readback::texture(chunk_image.image.clone()));
            }
            (false, Some(_)) => {
                // Remove Readback if chunk is not in view but has it
                commands.entity(entity).remove::<Readback>();
            }
            _ => {} // No action needed for other cases
        }
    }
}

pub fn update_chunk_array_indices(
    fow_screen: Res<FogOfWarScreen>,
    mut query: Query<(&ChunkCoord, &mut ChunkArrayIndex)>,
) {
    let chunks_per_row = (fow_screen.screen_size.x / fow_screen.chunk_size).ceil() as i32 + 5;
    
    for (coord, mut array_index) in query.iter_mut() {
        let rel_chunk_x = coord.x - fow_screen.view_start_chunk.x as i32;
        let rel_chunk_y = coord.y - fow_screen.view_start_chunk.y as i32;
        array_index.index = rel_chunk_y * chunks_per_row + rel_chunk_x;
    }
}
