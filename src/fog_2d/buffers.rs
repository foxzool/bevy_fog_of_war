use bevy::prelude::{Changed, Commands, Entity, GlobalTransform, Query, RemovedComponents, Res, ResMut, Resource};
use bevy::render::Extract;
use bevy::render::render_resource::StorageBuffer;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::utils::{Entry, HashMap};
use crate::FogSight2D;

#[derive(Resource)]
pub(super) struct ExtractedSight2DBuffers {
    changed: Vec<(Entity, FogSight2D)>,
    removed: Vec<Entity>,
}

pub(super) fn extract_buffers(
    mut commands: Commands,
    changed: Extract<Query<(Entity, &FogSight2D, &GlobalTransform), Changed<FogSight2D>>>,
    mut removed: Extract<RemovedComponents<FogSight2D>>,
) {
    commands.insert_resource(ExtractedSight2DBuffers {
        changed: changed
            .iter()
            .map(|(entity, settings, transform)| {
                let mut settings = settings.clone();
                settings.position = transform.translation().truncate();
                (entity, settings)
            })
            .collect(),
        removed: removed.read().collect(),
    });
}

#[derive(Resource, Default)]
pub(super) struct FogSight2dBuffers {
    pub(super) sights: HashMap<Entity, FogSight2D>,
    pub(super) buffers: StorageBuffer<Vec<FogSight2D>>,
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
