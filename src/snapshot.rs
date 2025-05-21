use crate::prelude::*;
use crate::render::{RenderSnapshotTempTexture, RenderSnapshotTexture};
use bevy::core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy::render::RenderApp;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{Node, NodeRunError, RenderGraphContext};
use bevy::render::render_graph::{RenderGraphApp, RenderLabel};
use bevy::render::render_resource::{Extent3d, Origin3d, TexelCopyTextureInfo, TextureAspect};
use bevy::render::renderer::RenderContext;
use bevy::render::texture::GpuImage;

pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<SnapshotCameraState>::default());
        app.init_resource::<SnapshotCameraState>();
        app.add_systems(PostUpdate, prepare_snapshot_camera);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.add_render_graph_node::<SnapshotNode>(Core2d, SnapshotNodeLabel);
        render_app.add_render_graph_edge(Core2d, SnapshotNodeLabel, Node2d::StartMainPass);

        // render_app.add_render_graph_edges(
        //     Core2d,
        //     (
        //         Node2d::MainTransparentPass,
        //         SnapshotNodeLabel,
        //         crate::render::FogComputeNodeLabel,
        //         crate::render::FogOverlayNodeLabel,
        //         Node2d::EndMainPass,
        //     ),
        // );
    }
}

/// Prepares and configures the single SnapshotCamera entity for the current frame's snapshot request (if any).
/// 为当前帧的快照请求（如果有）准备和配置单个 SnapshotCamera 实体。
fn prepare_snapshot_camera(
    mut snapshot_requests: ResMut<MainWorldSnapshotRequestQueue>,
    snapshot_camera_query: Single<(&mut Camera, &mut GlobalTransform), With<SnapshotCamera>>,
    mut snapshot_camera_state: ResMut<SnapshotCameraState>,
) {
    // Process one request per frame for simplicity
    let (mut camera, mut global_transform) = snapshot_camera_query.into_inner();
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

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct SnapshotNodeLabel;

#[derive(Default)]
pub struct SnapshotNode;

impl Node for SnapshotNode {
    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let camera_state = world.resource::<SnapshotCameraState>();
        if let Some(layer_index) = camera_state.snapshot_layer_index {
            let gpu_images = world.resource::<RenderAssets<GpuImage>>();
            let render_snapshot_temp_texture = world.resource::<RenderSnapshotTempTexture>();
            let render_snapshot_texture = world.resource::<RenderSnapshotTexture>();

            let Some(snapshot_temp_image) = gpu_images.get(&render_snapshot_temp_texture.0) else {
                return Ok(());
            };

            let Some(snapshot_images) = gpu_images.get(&render_snapshot_texture.0) else {
                return Ok(());
            };

            render_context.command_encoder().copy_texture_to_texture(
                snapshot_temp_image.texture.as_image_copy(),
                TexelCopyTextureInfo {
                    texture: &snapshot_images.texture,
                    mip_level: 0,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: layer_index,
                    },
                    aspect: TextureAspect::All,
                },
                Extent3d {
                    width: snapshot_temp_image.size.width,
                    height: snapshot_temp_image.size.height,
                    depth_or_array_layers: 1,
                },
            );

            info!(
                "Copying temp texture to snapshot layer {}. Temp texture size: {}x{}, format: {:?}",
                layer_index,
                snapshot_temp_image.size.width,
                snapshot_temp_image.size.height,
                snapshot_temp_image.texture_format
            );
        }

        Ok(())
    }
}
