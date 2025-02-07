use crate::{FogOfWarScreen, FogOfWarSettings, DEBUG};
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use std::collections::HashMap;

pub const CHUNK_SIZE: f32 = 256.;

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
    pub current: Option<i32>,
    pub previous: Option<i32>,
    pub ring_buffer_position: Option<(i32, i32)>, // 在环形缓存中的位置 (x, y)
}

impl ChunkArrayIndex {
    pub fn require_chunk_transport(&self) -> bool {
        self.previous.is_some() && self.current != self.previous
    }

    pub fn update_ring_buffer_position(
        &mut self,
        new_x: i32,
        new_y: i32,
        buffer_width: i32,
        buffer_height: i32,
    ) -> Option<i32> {
        let x = new_x.rem_euclid(buffer_width);
        let y = new_y.rem_euclid(buffer_height);
        self.ring_buffer_position = Some((x, y));
        Some(y * buffer_width + x)
    }
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
                    if DEBUG {
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
    if fow_screen.screen_size == Vec2::ZERO {
        return;
    }
    // 计算相机位置对应的chunk坐标
    let camera_chunk_x = (fow_screen.camera_position.x / fow_screen.chunk_size).floor() as i32;
    let camera_chunk_y = (fow_screen.camera_position.y / fow_screen.chunk_size).floor() as i32;

    // 计算环形缓存的大小（比视口大2行2列）
    let (chunks_x, chunks_y) = fow_screen.calculate_max_chunks();
    let buffer_width = chunks_x as i32 + 2;
    let buffer_height = chunks_y as i32 + 2;

    for (coord, mut array_index) in query.iter_mut() {
        // 计算chunk在环形缓存中的相对位置
        let relative_x = coord.x - (camera_chunk_x - buffer_width / 2);
        let relative_y = coord.y - (camera_chunk_y - buffer_height / 2);

        // 如果chunk在视野范围内（考虑额外的缓冲区）
        if relative_x >= -1
            && relative_x <= buffer_width
            && relative_y >= -1
            && relative_y <= buffer_height
        {
            // 保存旧的索引
            array_index.previous = array_index.current;

            // 更新环形缓存中的位置和索引
            array_index.current = array_index.update_ring_buffer_position(
                relative_x,
                relative_y,
                buffer_width,
                buffer_height,
            );

            if array_index.current != array_index.previous {
                debug!(
                    "{:?} index update {:?} => {:?} at ring buffer pos {:?}",
                    coord,
                    array_index.previous,
                    array_index.current,
                    array_index.ring_buffer_position
                );
            }
        } else {
            // 如果chunk不在视野范围内，清除其索引
            array_index.previous = array_index.current;
            array_index.current = None;
            array_index.ring_buffer_position = None;
        }
    }
}

pub fn debug_chunk_indices(
    fow_screen: Res<FogOfWarScreen>,
    chunks_query: Query<(&ChunkArrayIndex, &ChunkCoord, &Children)>,
    mut text_query: Query<&mut Text2d>,
) {
    if DEBUG {
        for (chunk_index, chunk_coord, children) in chunks_query.iter() {
            for child in children.iter() {
                let mut text = text_query.get_mut(*child).unwrap();
                text.0 = format!(
                    "({}, {})[{}/{}]",
                    chunk_coord.to_world_pos().x,
                    chunk_coord.to_world_pos().y,
                    chunk_index.previous.unwrap_or_default(),
                    chunk_index.current.unwrap_or_default()
                );
            }
        }
    }
}

#[derive(Component)]
pub struct ChunkDebugText;
