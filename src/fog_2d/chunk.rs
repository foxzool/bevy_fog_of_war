use crate::{FogOfWarScreen, FogOfWarSettings};
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use std::collections::HashMap;

pub const CHUNK_SIZE: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, ExtractComponent)]
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

#[derive(Component, Deref, DerefMut, Default)]
pub struct ChunkCache(pub Vec<u8>);

#[derive(Component, Default, ExtractComponent, Clone, Debug)]
pub struct ChunkArrayIndex {
    pub index: Option<u32>,
    pub previous_index: Option<u32>,
}

pub fn update_chunks_system(
    fow_screen: Res<FogOfWarScreen>,
    mut commands: Commands,
    mut chunks_query: Query<(Entity, &ChunkCoord, &ChunkCache)>,
) {
    let chunks_in_view = fow_screen.get_chunks_in_view();
    let mut existing_coords: Vec<ChunkCoord> =
        chunks_query.iter().map(|(_, coord, _)| *coord).collect();

    let text_font = TextFont {
        font_size: 20.0,
        ..default()
    };
    // Handle chunk loading for new chunks
    for coord in chunks_in_view.iter() {
        if !existing_coords.contains(coord) {
            let world_pos = coord.to_world_pos();
            debug!("spawn coord: {:?} {:?}", coord, coord.to_world_pos());
            commands
                .spawn((
                    *coord,
                    ChunkCache::default(),
                    ChunkArrayIndex::default(),
                    Transform::from_xyz(world_pos.x, world_pos.y, 0.0),
                ))
                .with_children(|p| {
                    if fow_screen.can_debug() {
                        p.spawn((Text2d::default(), text_font.clone(), ChunkDebugText));
                    }
                });
        }
    }
}

pub fn update_chunk_array_indices(
    fow_screen: Res<FogOfWarScreen>,
    mut query: Query<(&ChunkCoord, &mut ChunkArrayIndex)>,
) {
    // 计算相机位置对应的chunk坐标
    let camera_chunk_x = (fow_screen.camera_position.x / fow_screen.chunk_size).floor() as i32;
    let camera_chunk_y = (fow_screen.camera_position.y / fow_screen.chunk_size).floor() as i32;

    // 修改为 -1 保持对称padding（与WGSL代码同步）
    let top_left_chunk_x = camera_chunk_x - 1;
    let top_left_chunk_y = camera_chunk_y - 1;

    // 修改为 +2 保持对称（视口宽度 + 左右各1块padding）
    let chunks_per_row = (fow_screen.screen_size.x / fow_screen.chunk_size).ceil() as i32 + 2;

    for (coord, mut array_index) in query.iter_mut() {
        // 保存旧的索引
        array_index.previous_index = array_index.index;

        // 计算相对于视口左上角的坐标
        let relative_x = coord.x - top_left_chunk_x;
        let relative_y = coord.y - top_left_chunk_y;

        // 计算新的数组索引
        let chunk_index = relative_y * chunks_per_row + relative_x;
        array_index.index = if chunk_index >= 0 {
            Some(chunk_index as u32)
        } else {
            None
        };
    }
}

pub fn debug_chunk_indices(
    fow_screen: Res<FogOfWarScreen>,
    chunks_query: Query<(&ChunkArrayIndex, &ChunkCoord, &Children)>,
    mut text_query: Query<&mut Text2d>,
) {
    if fow_screen.can_debug() {
        for (chunk_index, chunk_coord, children) in chunks_query.iter() {
            for child in children.iter() {
                let mut text = text_query.get_mut(*child).unwrap();
                text.0 = format!(
                    "({}, {})[{}]",
                    chunk_coord.x,
                    chunk_coord.y,
                    chunk_index.index.unwrap_or_default()
                );
            }
        }
    }
}

#[derive(Component)]
pub struct ChunkDebugText;
