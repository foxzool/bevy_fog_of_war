use crate::fog_2d::chunk::{get_chunks_in_rect, ChunkCoord, ChunkRingBuffer};
use crate::fog_2d::pipeline::FogOfWar2dPipeline;
use crate::{FogOfWarCamera, FogSight2D};
use crate::{FogOfWarSettings, FogSight2DUniform};
use bevy::math::{Vec2, Vec3};
use bevy::prelude::{
    debug, Changed, Commands, Entity, GlobalTransform, Query, RemovedComponents, Res, ResMut,
    Resource, Single, Transform, Vec3Swizzles, Window, With,
};
use bevy::render::camera::Camera;
use bevy::render::render_resource::{StorageBuffer, UniformBuffer};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::Extract;
use bevy::utils::{Entry, HashMap};
use bevy::window::PrimaryWindow;
use bevy_render::prelude::OrthographicProjection;
use bevy_render::render_resource::ShaderType;

#[derive(Resource)]
pub(super) struct ExtractedSight2DBuffers {
    changed: Vec<(Entity, FogSight2DUniform)>,
    removed: Vec<Entity>,
}

pub(super) fn extract_buffers(
    mut commands: Commands,
    changed: Extract<Query<(Entity, &FogSight2D, &GlobalTransform), Changed<FogSight2D>>>,
    mut removed: Extract<RemovedComponents<FogSight2D>>,
    camera_query: Extract<Query<(&Camera, &GlobalTransform)>>,
) {
    let (camera, camera_transform) = if let Ok(cam) = camera_query.get_single() {
        cam
    } else {
        return;
    };

    // Get camera position in world space
    let camera_pos = camera_transform.translation().truncate();

    let mut removed_entities = removed.read().collect::<Vec<_>>();
    let changed_entities: Vec<_> = changed
        .iter()
        .filter_map(|(entity, settings, transform)| {
            let world_pos = transform.translation();
            if is_visible_to_camera(world_pos, settings.radius, camera, camera_transform) {
                // Calculate position relative to screen space

                Some((
                    entity,
                    FogSight2DUniform {
                        position: world_pos.truncate(),
                        radius: settings.radius,
                    },
                ))
            } else {
                removed_entities.push(entity);
                None
            }
        })
        .collect();

    commands.insert_resource(ExtractedSight2DBuffers {
        changed: changed_entities,
        removed: removed_entities,
    });
}

// Helper function to check if a point is visible to the camera
fn is_visible_to_camera(
    point: Vec3,
    radius: f32,
    camera: &Camera,
    camera_transform: &GlobalTransform,
) -> bool {
    let view_matrix = camera_transform.compute_matrix();
    let point_in_view = view_matrix.inverse().transform_point3(point);

    if point_in_view.z < 0.0 {
        return false;
    }

    if let Some(viewport_size) = camera.logical_viewport_size() {
        let half_width = viewport_size.x * 0.5;
        let half_height = viewport_size.y * 0.5;

        // Check if any part of the sight circle intersects with the viewport
        let min_x = point_in_view.x - radius;
        let max_x = point_in_view.x + radius;
        let min_y = point_in_view.y - radius;
        let max_y = point_in_view.y + radius;

        // If any part of the sight's bounding box overlaps with the viewport, consider it visible
        (min_x <= half_width && max_x >= -half_width)
            && (min_y <= half_height && max_y >= -half_height)
    } else {
        false
    }
}

#[derive(Resource, Default)]
pub struct FogSight2dBuffers {
    pub sights: HashMap<Entity, FogSight2DUniform>,
    pub buffers: StorageBuffer<Vec<FogSight2DUniform>>,
}

#[derive(Resource, Default)]
pub struct FogOfWarSettingBuffer {
    pub buffer: UniformBuffer<FogOfWarSettings>,
}

#[derive(Resource, Default)]
pub struct FogOfWarRingBuffers {
    pub buffers: StorageBuffer<Vec<RingBuffer>>,
}

#[derive(Clone, Default, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
#[repr(C)]
pub struct RingBuffer {
    pub position: Vec2,
    pub index: i32,
}

pub fn prepare_settings_buffer(
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    settings: Res<FogOfWarSettings>,
    mut settings_buffer: ResMut<FogOfWarSettingBuffer>,
) {
    settings_buffer.buffer = UniformBuffer::from(settings.clone());
    settings_buffer.buffer.write_buffer(&device, &queue);
}

pub(super) fn prepare_buffers(
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut extracted: ResMut<ExtractedSight2DBuffers>,
    mut buffer_res: ResMut<FogSight2dBuffers>,
) {
    for (entity, fog_sight_2d) in extracted.changed.drain(..) {
        match buffer_res.sights.entry(entity) {
            Entry::Occupied(mut entry) => {
                let value = entry.get_mut();
                *value = fog_sight_2d;
            }
            Entry::Vacant(entry) => {
                entry.insert(fog_sight_2d);
            }
        }
    }

    for entity in extracted.removed.drain(..) {
        buffer_res.sights.remove(&entity);
    }

    let sights: Vec<_> = buffer_res.sights.values().cloned().collect();
    buffer_res.buffers = StorageBuffer::from(sights);
    buffer_res.buffers.write_buffer(&device, &queue);
}

pub fn prepare_chunk_texture(
    settings: Res<FogOfWarSettings>,
    fog_of_war_pipeline: ResMut<FogOfWar2dPipeline>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &OrthographicProjection, &GlobalTransform), With<FogOfWarCamera>>,
    mut chunks_query: Query<(&ChunkCoord, &mut ChunkRingBuffer, &GlobalTransform)>,
    mut ring_res: ResMut<FogOfWarRingBuffers>,
) {
    let Ok((_camera, projection, global_transform)) = cameras.get_single() else {
        return;
    };
    // 获取当前视野内的chunks
    let chunks_in_view = get_chunks_in_rect(projection.area, global_transform, settings.chunk_size);

    let buffers = chunks_query
        .iter()
        .filter_map(|(_, ring_buffer, global_transform)| {
            let Some(current) = ring_buffer.current else {
                return None;
            };
            let pos = global_transform.translation().truncate();
            Some(RingBuffer {
                position: pos,
                index: current,
            })
        })
        .collect::<Vec<RingBuffer>>();

    ring_res.buffers = StorageBuffer::from(buffers);
    ring_res.buffers.write_buffer(&device, &queue);

    // FIXME
    return;

    // 遍历所有已存在的chunks
    for (coord, mut ring_buffer, _) in chunks_query.iter_mut() {
        // 如果chunk不在视野内，清空其纹理
        if !chunks_in_view.contains(coord) {
            if let (Some(index), Some(prev_index)) = (ring_buffer.current, ring_buffer.previous) {
                // 应该同时清空新旧两个索引的纹理
                debug!("{:?} clean both {} {}", coord, index, prev_index);
                fog_of_war_pipeline.clear_explored_texture(&queue, index, settings.chunk_size);
                fog_of_war_pipeline.clear_explored_texture(&queue, prev_index, settings.chunk_size);
                ring_buffer.previous = None;
            }
            // 处理只有当前索引的情况
            else if let Some(index) = ring_buffer.current {
                debug!("{:?} clean {}", coord, index);
                fog_of_war_pipeline.clear_explored_texture(&queue, index, settings.chunk_size);
                ring_buffer.current = None;
            }
        } else if ring_buffer.require_chunk_transport() {
            // 如果chunk的索引发生变化，需要转移数据
            debug!("{:?} transfer {:?}", coord, ring_buffer);
            if let (Some(index), Some(prev_index)) = (ring_buffer.current, ring_buffer.previous) {
                fog_of_war_pipeline.transfer_chunk_data(
                    &device,
                    &queue,
                    prev_index,
                    index,
                    settings.chunk_size,
                );
            }
        }
    }
}
