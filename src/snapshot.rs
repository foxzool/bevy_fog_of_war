use crate::prelude::*;
use crate::render::{RenderSnapshotTempTexture, RenderSnapshotTexture};
use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy::render::camera::RenderTarget;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{Node, NodeRunError, RenderGraphContext};
use bevy::render::render_graph::{RenderGraphApp, RenderLabel};
use bevy::render::render_resource::{
    Extent3d, Origin3d, TexelCopyTextureInfo, TextureAspect, TextureDimension, TextureUsages,
};
use bevy::render::renderer::RenderContext;
use bevy::render::texture::GpuImage;
use bevy::render::view::RenderLayers;
use bevy::render::{Extract, RenderApp};

pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<SnapshotCameraState>::default());
        app.init_resource::<SnapshotCameraState>();
        app.add_systems(Startup, setup_snapshot_camera)
            .add_systems(PostUpdate, prepare_snapshot_camera)
            .add_systems(Update, ensure_snapshot_render_layer)
            .add_systems(Last, check_snapshot_image_ready);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.add_render_graph_node::<SnapshotNode>(Core2d, SnapshotNodeLabel);
        // render_app.add_render_graph_edge(Core2d, SnapshotNodeLabel, Node2d::MainTransparentPass);

        render_app.add_render_graph_edges(
            Core2d,
            (
                Node2d::MainTransparentPass,
                SnapshotNodeLabel,
                crate::render::FogComputeNodeLabel,
                crate::render::FogOverlayNodeLabel,
                Node2d::EndMainPass,
            ),
        );
    }
}

/// 标记组件，指示该实体应被包含在战争迷雾的快照中
/// Marker component indicating this entity should be included in the fog of war snapshot
#[derive(Component, Debug, Clone, Default, Reflect)]
#[reflect(Component, Default)]
pub struct Capturable;

/// Marker component for a camera used to render snapshots.
/// 用于渲染快照的相机的标记组件。
#[derive(Component, ExtractComponent, Clone, Default, Reflect)]
#[reflect(Component)]
pub struct SnapshotCamera;

#[derive(Component)]
pub struct ActiveSnapshotTarget {
    pub snapshot_layer_index: u32,
    pub world_bounds: Rect, // For reference, projection is set based on this
}

pub const SNAPSHOT_RENDER_LAYER_ID: usize = 7;

pub const SNAPSHOT_RENDER_LAYER: RenderLayers = RenderLayers::layer(SNAPSHOT_RENDER_LAYER_ID);

/// Prepares and configures the single SnapshotCamera entity for the current frame's snapshot request (if any).
/// 为当前帧的快照请求（如果有）准备和配置单个 SnapshotCamera 实体。
fn prepare_snapshot_camera(
    mut snapshot_requests: ResMut<MainWorldSnapshotRequestQueue>,
    snapshot_camera_query: Single<(&mut Camera, &mut GlobalTransform), With<SnapshotCamera>>,
    mut snapshot_camera_state: ResMut<SnapshotCameraState>,
) {
    if snapshot_camera_state.capturing {
        return;
    }
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
        snapshot_camera_state.capturing = true;
        snapshot_camera_state.frame_to_wait = 2;
    }
}

fn setup_snapshot_camera(
    mut commands: Commands,
    settings: Res<FogMapSettings>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut snapshot_temp_image = Image::new_fill(
        Extent3d {
            width: settings.texture_resolution_per_chunk.x,
            height: settings.texture_resolution_per_chunk.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0; 4],
        settings.snapshot_texture_format,
        RenderAssetUsages::default(),
    );
    snapshot_temp_image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT // To render snapshots into / 用于渲染快照
        | TextureUsages::TEXTURE_BINDING // For sampling in overlay shader / 用于在覆盖 shader 中采样
        | TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
        | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输

    let snapshot_temp_handle = images.add(snapshot_temp_image);

    commands.insert_resource(SnapshotTempTexture {
        handle: snapshot_temp_handle.clone(),
    });
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: settings.chunk_size.x as f32 / settings.texture_resolution_per_chunk.x as f32,
            scaling_mode: bevy::render::camera::ScalingMode::Fixed {
                width: settings.texture_resolution_per_chunk.x as f32,
                height: settings.texture_resolution_per_chunk.y as f32,
            },
            ..OrthographicProjection::default_2d()
        }),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            order: -1,       // Render before the main camera, or as needed by graph
            is_active: true, // Initially inactive
            hdr: false,      // Snapshots likely don't need HDR
            target: RenderTarget::Image(snapshot_temp_handle.clone().into()),
            ..default()
        },
        SnapshotCamera, // Mark it as our snapshot camera
        SNAPSHOT_RENDER_LAYER,
    ));
}

fn check_snapshot_image_ready(
    mut snapshot_camera_state: ResMut<SnapshotCameraState>,
    snapshot_camera_query: Single<&mut Camera, With<SnapshotCamera>>,
) {
    if snapshot_camera_state.capturing {
        if snapshot_camera_state.frame_to_wait > 0 {
            snapshot_camera_state.frame_to_wait -= 1;
            return;
        }

        if snapshot_camera_state.frame_to_wait == 0 {
            let mut camera = snapshot_camera_query.into_inner();
            camera.is_active = false;
            snapshot_camera_state.snapshot_layer_index = None;
            snapshot_camera_state.capturing = false;
        }
    }
}

/// Resource to manage the state of the snapshot camera entity in the RenderWorld.
/// 用于管理 RenderWorld 中快照相机实体状态的资源。
#[derive(Resource, ExtractResource, Clone, Default)]
pub struct SnapshotCameraState {
    pub capturing: bool,
    pub frame_to_wait: u8,
    pub snapshot_layer_index: Option<u32>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct SnapshotNodeLabel;

#[derive(Default)]
pub struct SnapshotNode;

impl Node for SnapshotNode {
    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let view_entity = graph.view_entity();

        if world.get::<SnapshotCamera>(view_entity).is_none() {
            return Ok(());
        }

        let camera_state = world.resource::<SnapshotCameraState>();
        if let Some(layer_index) = camera_state.snapshot_layer_index {
            if camera_state.frame_to_wait > 0 {
                return Ok(());
            }
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

            trace!(
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

pub fn ensure_snapshot_render_layer(
    mut commands: Commands,
    snapshot_visible_query: Query<(Entity, Option<&RenderLayers>), With<Capturable>>,
) {
    for (entity, existing_layers) in snapshot_visible_query.iter() {
        let snapshot_layer = SNAPSHOT_RENDER_LAYER.clone();
        let combined_layers = match existing_layers {
            Some(layers) => layers.union(&snapshot_layer),
            None => snapshot_layer,
        };

        commands.entity(entity).insert((
            combined_layers, // Ensure it's on the snapshot layer
        ));
    }
}
