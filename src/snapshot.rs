use crate::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};

pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<SnapshotCameraState>::default());
        app.init_resource::<SnapshotCameraState>();
        app.add_systems(PostUpdate, prepare_snapshot_camera);
    }
}

/// Prepares and configures the single SnapshotCamera entity for the current frame's snapshot request (if any).
/// 为当前帧的快照请求（如果有）准备和配置单个 SnapshotCamera 实体。
fn prepare_snapshot_camera(
    mut snapshot_requests: ResMut<MainWorldSnapshotRequestQueue>,
    snapshot_camera_query: Single<
        (&mut Camera, &mut GlobalTransform, &mut Projection),
        With<SnapshotCamera>,
    >,
    mut snapshot_camera_state: ResMut<SnapshotCameraState>,
) {
    // Process one request per frame for simplicity
    let (mut camera, mut global_transform, mut projection) = snapshot_camera_query.into_inner();
    if let Some(request) = snapshot_requests.requests.pop() {
        // Take one request
        debug!(
            "Preparing snapshot camera for layer {} at {:?} ",
            request.snapshot_layer_index,
            request.world_bounds.center()
        );
        let center = request.world_bounds.center();
        let transform = Transform::from_xyz(center.x, center.y, 999.0); // Ensure Z is appropriate
        *global_transform = GlobalTransform::from(transform);
        camera.is_active = true;
        snapshot_camera_state.snapshot_layer_index = Some(request.snapshot_layer_index);
        // *projection = Projection::Orthographic(OrthographicProjection {
        //     area: Rect {
        //         min: Vec2::ZERO,
        //         max: Vec2::splat(512.0),
        //     },
        //     ..OrthographicProjection::default_2d()
        // });
    } else {
        camera.is_active = false;
        snapshot_camera_state.snapshot_layer_index = None;
    }
}

/// Resource to manage the state of the snapshot camera entity in the RenderWorld.
/// 用于管理 RenderWorld 中快照相机实体状态的资源。
#[derive(Resource, ExtractResource, Clone, Default)]
pub struct SnapshotCameraState {
    pub snapshot_layer_index: Option<u32>,
}
