use crate::FogOfWarSettings;
use crate::{calculate_max_chunks, FogOfWarCamera};
use bevy::color::palettes::basic::{BLUE, YELLOW};
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::{PrimaryWindow, WindowResized};
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
        let x = if world_pos.x >= 0.0 {
            (world_pos.x / chunk_size) as i32
        } else {
            ((world_pos.x - chunk_size + 1.0) / chunk_size).ceil() as i32
        };
        let y = if world_pos.y >= 0.0 {
            ((world_pos.y + chunk_size - 1.0) / chunk_size) as i32
        } else {
            (world_pos.y / chunk_size).ceil() as i32
        };
        Self { x, y }
    }
}

#[derive(Component, Default, ExtractComponent, Clone, Debug)]
pub struct ChunkRingBuffer {
    pub current: Option<i32>,
    pub previous: Option<i32>,
    pub ring_buffer_position: Option<(i32, i32)>, // 在环形缓存中的位置 (x, y)
}

impl ChunkRingBuffer {
    pub fn new(current: i32) -> Self {
        Self {
            current: Some(current),
            previous: None,
            ring_buffer_position: None,
        }
    }

    pub fn set_current(&mut self, current: i32) {
        self.previous = self.current;
        self.current = Some(current);
        self.ring_buffer_position = None;
    }

    pub fn clean_current(&mut self) {
        self.current = None;
        self.previous = None;
        self.ring_buffer_position = None;
    }

    pub fn require_chunk_transport(&self) -> bool {
        self.previous.is_some() && self.current != self.previous
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

    for (entity, chunk_coord, mut chunk_ring_buffer) in chunks_query.iter_mut() {
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
        debug!(
            "spawn coord: {:?} {} {:?}",
            coord,
            i,
            coord.to_world_pos(settings.chunk_size)
        );
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

pub fn update_chunk_ring_buffer(
    fow_settings: Res<FogOfWarSettings>,
    mut query: Query<(&ChunkCoord, &mut ChunkRingBuffer)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform)>,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera.get_single() else {
        return;
    };

    let Ok(viewport_center) = camera.viewport_to_world_2d(camera_transform, window.size() * 0.5)
    else {
        return;
    };

    let camera_chunk_x = (viewport_center.x / fow_settings.chunk_size).ceil() as i32;
    let camera_chunk_y = (viewport_center.y / fow_settings.chunk_size).ceil() as i32;

    // 计算环形缓存的大小（比视口大2行2列）
    let (chunks_x, chunks_y) = calculate_max_chunks(
        Vec2::new(
            window.resolution.physical_width() as f32,
            window.resolution.physical_height() as f32,
        ),
        fow_settings.chunk_size,
    );
    let buffer_width = chunks_x as i32 + 2;
    let buffer_height = chunks_y as i32 + 2;

    // 计算视口的左上角chunk坐标
    let viewport_start_x = camera_chunk_x - buffer_width / 2;
    let viewport_start_y = camera_chunk_y + buffer_height / 2; // 注意这里改为加号，因为我们要从上往下计数

    for (coord, mut array_index) in query.iter_mut() {
        // 计算chunk相对于视口左上角的偏移
        let relative_x = coord.x - viewport_start_x;
        let relative_y = viewport_start_y - coord.y; // 注意这里改为减法，反转y轴方向

        // 如果chunk在视野范围内（考虑额外的缓冲区）
        if relative_x >= 0
            && relative_x < buffer_width
            && relative_y >= 0
            && relative_y < buffer_height
        {
            // 保存旧的索引
            array_index.previous = array_index.current;

            // 计算环形缓存中的位置
            let x = relative_x;
            let y = relative_y;
            array_index.ring_buffer_position = Some((x, y));

            // 从左上到右下计算索引
            array_index.current = Some(y * buffer_width + x);

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
