use crate::fog_2d::pipeline::FogOfWar2dPipeline;
use crate::FogSight2DUniform;
use crate::{FogOfWarScreen, FogSight2D};
use bevy::math::{Vec2, Vec3};
use bevy::prelude::{
    Changed, Commands, Entity, GlobalTransform, Query, RemovedComponents, Res, ResMut, Resource,
};
use bevy::render::camera::Camera;
use bevy::render::render_resource::{StorageBuffer, UniformBuffer};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::Extract;
use bevy::utils::{Entry, HashMap};
use crate::fog_2d::chunk::{ChunkArrayIndex, ChunkCoord};

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
                let relative_pos = world_pos.truncate() - camera_pos;

                Some((
                    entity,
                    FogSight2DUniform {
                        position: Vec2::new(relative_pos.x, -relative_pos.y),
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
pub(super) struct FogSight2dBuffers {
    pub(super) sights: HashMap<Entity, FogSight2DUniform>,
    pub(super) buffers: StorageBuffer<Vec<FogSight2DUniform>>,
}

#[derive(Resource, Default)]
pub(super) struct FogSight2dScreenBuffers {
    pub(super) buffers: UniformBuffer<FogOfWarScreen>,
}

pub(super) fn prepare_buffers(
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut extracted: ResMut<ExtractedSight2DBuffers>,
    mut buffer_res: ResMut<FogSight2dBuffers>,
    screen: Res<FogOfWarScreen>,
    mut screen_buffer: ResMut<FogSight2dScreenBuffers>,
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

    screen_buffer.buffers = UniformBuffer::from(screen.clone());
    screen_buffer.buffers.write_buffer(&device, &queue);
}

pub(super) fn prepare_chunk_texture(
    screen: Res<FogOfWarScreen>,
    mut screen_buffer: ResMut<FogSight2dScreenBuffers>,
    mut fog_of_war_pipeline: ResMut<FogOfWar2dPipeline>,
    queue: Res<RenderQueue>,
    chunks_query: Query<(&ChunkCoord, &ChunkArrayIndex)>,
) {
    // 获取当前视野内的chunks
    let chunks_in_view = screen.get_chunks_in_view();
    
    // 遍历所有已存在的chunks
    for (coord, array_index) in chunks_query.iter() {
        // 如果chunk不在视野内，清空其纹理
        if !chunks_in_view.contains(coord) {
            fog_of_war_pipeline.clear_explored_texture(&queue, array_index.index);
        }
    }
}
