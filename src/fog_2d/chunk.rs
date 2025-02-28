use crate::FogOfWarSettings;
use crate::{calculate_max_chunks, FogOfWarCamera};
use bevy::color::palettes::basic::{BLUE, YELLOW};
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::{PrimaryWindow, WindowResized};
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, ExtractComponent)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

impl ChunkCoord {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn to_world_pos(&self, chunk_size: f32) -> Vec2 {
        Vec2::new(self.x as f32 * chunk_size, self.y as f32 * chunk_size)
    }

    pub fn from_world_pos(world_pos: Vec2, chunk_size: f32) -> Self {
        let x = (world_pos.x / chunk_size).floor() as i32;
        let y = (world_pos.y / chunk_size).floor() as i32;
        Self { x, y }
    }
}

#[derive(Component, Default, ExtractComponent, Clone, Debug)]
pub struct ChunkRingBuffer {
    pub current: Option<i32>,
    pub previous: Option<i32>,
    pub ring_buffer_position: Option<(i32, i32)>, // 在环形缓存中的位置 (x, y)
    pub stable_index: Option<i32>,  // 稳定的索引，一旦分配就不会改变
}

impl Eq for ChunkRingBuffer {}

impl PartialEq<Self> for ChunkRingBuffer {
    fn eq(&self, other: &Self) -> bool {
       self.current == other.current
    }
}

impl PartialOrd<Self> for ChunkRingBuffer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.current.partial_cmp(&other.current)
    }
}

impl Ord for ChunkRingBuffer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.current.cmp(&other.current)
    }
}

impl ChunkRingBuffer {
    pub fn new(current: i32) -> Self {
        Self {
            current: Some(current),
            previous: None,
            ring_buffer_position: None,
            stable_index: None,
        }
    }

    pub fn visible(&self) -> bool {
        self.current.is_some()
    }

    pub fn set_current(&mut self, current: i32) {
        if self.stable_index.is_none() {
            self.stable_index = Some(current);
        }
        self.previous = self.current;
        self.current = Some(current);
        self.ring_buffer_position = None;
    }

    pub fn clean_current(&mut self) {
        self.previous = self.current;
        self.current = None;
        self.ring_buffer_position = None;
    }

    pub fn require_chunk_transport(&self) -> bool {
        if let (Some(current), Some(previous)) = (self.current, self.previous) {
            current != previous
        } else {
            false
        }
    }
}

pub fn update_chunks_system(
    settings: Res<FogOfWarSettings>,
    cameras: Query<(&OrthographicProjection, &GlobalTransform), With<FogOfWarCamera>>,
    mut commands: Commands,
    mut chunks_query: Query<(Entity, &ChunkCoord, &mut ChunkRingBuffer)>,
) {
    let Ok((projection, global_transform)) = cameras.get_single() else {
        return;
    };
    let chunks_in_view = get_chunks_in_rect(projection.area, global_transform, settings.chunk_size);

    let mut chunks_with_index = chunks_in_view.into_iter().enumerate().collect::<Vec<_>>();

    for (_entity, chunk_coord, mut chunk_ring_buffer) in chunks_query.iter_mut() {
        if let Some((i, _)) = chunks_with_index
            .iter()
            .position(|(_, coord)| *coord == *chunk_coord)
            .map(|i| chunks_with_index.swap_remove(i))
        {
            chunk_ring_buffer.set_current(i as i32);
        } else {
            chunk_ring_buffer.clean_current();
        }
    }

    // Handle chunk loading for new chunks
    for (i, coord) in chunks_with_index.iter() {
        let world_pos = coord.to_world_pos(settings.chunk_size);
        debug!("spawn coord: ({}, {}) {}", coord.x, coord.y, i);
        commands
            .spawn((
                *coord,
                ChunkRingBuffer::new(*i as i32),
                Transform::from_xyz(world_pos.x, world_pos.y, 0.0),
            ))
            .with_children(|p| {
                if cfg!(feature = "debug_chunk") {
                    p.spawn((
                        Text2d::default(),
                        TextFont {
                            font_size: 16.0,
                            ..default()
                        },
                        ChunkDebugText,
                        TextLayout::new_with_justify(JustifyText::Left),
                        Transform::from_xyz(100.0, -20.0, 0.0),
                    ));
                }
            });
    }
}

pub fn debug_chunk_indices(
    chunks_query: Query<(&ChunkRingBuffer, &ChunkCoord, &Children)>,
    settings: Res<FogOfWarSettings>,
    mut text_query: Query<&mut Text2d>,
    mut gizmos: Gizmos,
) {
    for (chunk_index, chunk_coord, children) in chunks_query.iter() {
        let world_pos = chunk_coord.to_world_pos(settings.chunk_size);
        for child in children.iter() {
            let mut text = text_query.get_mut(*child).unwrap();
            text.0 = format!(
                "({}, {})[{}, {}] {}",
                chunk_coord.x,
                chunk_coord.y,
                world_pos.x,
                world_pos.y,
                chunk_index.current.unwrap_or_default()
            );
        }

        let chunk_size = settings.chunk_size;
        // gizmos.circle_2d(world_pos, 10.0, BLUE);
        // 使用左上角作为矩形的起点
        gizmos.rect_2d(
            Vec2::new(
                world_pos.x + chunk_size * 0.5,
                world_pos.y - chunk_size * 0.5,
            ), // 中心点需要偏移半个chunk大小
            Vec2::splat(chunk_size),
            YELLOW,
        );
    }
}

#[derive(Component)]
pub struct ChunkDebugText;

pub fn get_chunks_in_rect(
    area: Rect,
    global_transform: &GlobalTransform,
    chunk_size: f32,
) -> Vec<ChunkCoord> {
    let min_pos = global_transform
        .transform_point(Vec3::new(area.min.x - 1.0, area.min.y - 1.0, 0.0))
        .truncate();
    let max_pos = global_transform
        .transform_point(Vec3::new(area.max.x + 1.0, area.max.y + 1.0, 0.0))
        .truncate();
    // 找出所有角坐标中的最小/最大坐标
    let min_x = ChunkCoord::from_world_pos(min_pos, chunk_size).x;
    let min_y = ChunkCoord::from_world_pos(min_pos, chunk_size).y;
    let max_x = ChunkCoord::from_world_pos(max_pos, chunk_size).x;
    let max_y = ChunkCoord::from_world_pos(max_pos, chunk_size).y;

    // 生成所有可能的区块坐标组合
    let mut chunks = Vec::new();
    for y in (min_y..=max_y).rev() {
        for x in min_x..=max_x {
            chunks.push(ChunkCoord::new(x, y));
        }
    }
    chunks
}

#[test]
fn test_wold_pos() {
    let chunk_pos = ChunkCoord::from_world_pos(Vec2::new(0.0, 0.0), 256.0);
    assert_eq!(chunk_pos.x, 0);
    assert_eq!(chunk_pos.y, 0);

    let chunk_pos = ChunkCoord::from_world_pos(Vec2::new(641.0, 361.0), 256.0);
    assert_eq!(chunk_pos.x, 2);
    assert_eq!(chunk_pos.y, 2);

    let chunk_pos = ChunkCoord::from_world_pos(Vec2::new(256.0, 256.0), 256.0);
    assert_eq!(chunk_pos.x, 1);
    assert_eq!(chunk_pos.y, 1);

    let chunk_pos = ChunkCoord::from_world_pos(Vec2::new(-641.0, -361.0), 256.0);
    assert_eq!(chunk_pos.x, -3);
    assert_eq!(chunk_pos.y, -1);
}
