use crate::FogSight2DUniform;
use bevy::prelude::{Changed, Commands, Entity, GlobalTransform, Query, RemovedComponents, Res, ResMut, Resource};
use bevy::render::Extract;
use bevy::render::render_resource::StorageBuffer;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::utils::{Entry, HashMap};
use crate::FogSight2D;
use bevy::render::camera::Camera;
use bevy::math::Vec3;

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
    let camera = if let Some((camera, camera_transform)) = camera_query.get_single().ok() {
        (camera, camera_transform)
    } else {
        return;
    };

    let mut removed_entities = removed.read().collect::<Vec<_>>();
    let changed_entities: Vec<_> = changed
        .iter()
        .filter_map(|(entity, settings, transform)| {
            let position = transform.translation();
            if is_visible_to_camera(position, camera.0, camera.1) {
                Some((entity, FogSight2DUniform {
                    position: position.truncate(),
                    radius: settings.radius,
                }))
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
        
        point_in_view.x >= -half_width 
            && point_in_view.x <= half_width
            && point_in_view.y >= -half_height
            && point_in_view.y <= half_height
    } else {
        false
    }
}

#[derive(Resource, Default)]
pub(super) struct FogSight2dBuffers {
    pub(super) sights: HashMap<Entity, FogSight2DUniform>,
    pub(super) buffers: StorageBuffer<Vec<FogSight2DUniform>>,
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
