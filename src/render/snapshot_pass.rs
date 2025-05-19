// src/render/snapshot_pass.rs

use crate::components::{ActiveSnapshotTarget, SnapshotCamera}; // SNAPSHOT_RENDER_LAYER_ID
use crate::render::extract::{
    RenderSnapshotTempTexture, RenderSnapshotTexture, SnapshotRequestQueue,
};
use bevy::core_pipeline::core_2d::AlphaMask2d;
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::lifetimeless::Read;
use bevy::math::FloatOrd;
use bevy::render::camera::{ImageRenderTarget, RenderTarget};
use bevy::render::render_phase::ViewBinnedRenderPhases;
use bevy::render::render_resource::{
    CommandEncoderDescriptor, Extent3d, Origin3d, StoreOp, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::sprite::SpriteViewBindGroup;
use bevy::{
    core_pipeline::core_2d::{Camera2d, Opaque2d},
    prelude::*,
    render::{
        camera::ExtractedCamera,
        render_asset::RenderAssets,
        render_graph::{Node, NodeRunError, RenderGraphContext, RenderLabel, ViewNode},
        render_phase::TrackedRenderPass,
        render_resource::{
            LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, TextureView,
            TextureViewDescriptor,
        },
        renderer::RenderContext,
        texture::GpuImage,
        view::{ExtractedView, ViewTarget, ViewUniformOffset, ViewUniforms},
    },
};


/// Marker component for entities in the RenderWorld that should be part of snapshots.
/// RenderWorld 中应包含在快照中的实体的标记组件。
#[derive(Component, Clone, Copy)]
pub struct RenderWorldSnapshotVisible;

/// Resource to manage the state of the snapshot camera entity in the RenderWorld.
/// 用于管理 RenderWorld 中快照相机实体状态的资源。
#[derive(Resource, Default)]
pub struct SnapshotCameraState {
    pub entity: Option<Entity>,
    pub is_active_this_frame: bool,
    pub snapshot_layer_index: Option<u32>,
}

/// Prepares and configures the single SnapshotCamera entity for the current frame's snapshot request (if any).
/// 为当前帧的快照请求（如果有）准备和配置单个 SnapshotCamera 实体。
pub fn prepare_snapshot_camera(
    mut commands: Commands,
    mut camera_state: ResMut<SnapshotCameraState>,
    mut snapshot_requests: ResMut<SnapshotRequestQueue>, // Extracted requests
    // Query to find the existing SnapshotCamera entity
    snapshot_camera_query: Query<Entity, With<SnapshotCamera>>,
    render_snapshot_texture: Res<RenderSnapshotTempTexture>, // To set the nominal target
) {
    // Find or create the snapshot camera entity if not already stored
    if camera_state.entity.is_none() {
        if let Ok(entity) = snapshot_camera_query.single() {
            camera_state.entity = Some(entity);
        } else {
            // This should not happen if SnapshotCamera is spawned in setup_fog_resources
            error!("SnapshotCamera entity not found in RenderWorld!");
            return;
        }
    }

    let Some(camera_entity) = camera_state.entity else {
        return;
    };

    camera_state.is_active_this_frame = false; // Reset for the frame

    // Process one request per frame for simplicity
    if let Some(request) = snapshot_requests.requests.pop() {
        // Take one request
        // info!("Preparing snapshot camera for layer {} at bounds {:?} {}", request.snapshot_layer_index, request.world_bounds, camera_entity);
        let center = request.world_bounds.center();
        let size = request.world_bounds.size();

        // Configure the camera
        let projection = Projection::Orthographic(OrthographicProjection {
            far: 1000.0,
            near: -1000.0,
            scale: 1.0, // Adjust as needed, or keep 1.0 if world units are pixels
            area: request.world_bounds, // Use the chunk's bounds directly for the view area
            ..OrthographicProjection::default_2d()
        });

        let transform = Transform::from_xyz(center.x, center.y, 999.0); // Ensure Z is appropriate

        // Update the existing camera entity's components
        // The Camera component itself (target, order, hdr) should already be set up.
        // We primarily update its activity, projection, transform, and our ActiveSnapshotTarget.
        commands.entity(camera_entity).insert((
            projection,                       // OrthographicProjection
            GlobalTransform::from(transform), // Update GlobalTransform
            Camera {
                order: -1,       // Ensures it's processed for view extraction
                is_active: true, // Activate for this frame's snapshot
                hdr: false,
                target: RenderTarget::Image(ImageRenderTarget {
                    handle: render_snapshot_texture.0.clone(),
                    scale_factor: FloatOrd(1.0),
                }), // Nominally targets the whole array
                ..default() // Keep other camera defaults or prior settings
            },
            ActiveSnapshotTarget {
                // Our custom component
                snapshot_layer_index: request.snapshot_layer_index,
                world_bounds: request.world_bounds,
            },
            // Ensure it's on the correct render layer (already set at spawn, but good to be explicit)
            crate::components::SNAPSHOT_RENDER_LAYER.clone(),
        ));
        camera_state.is_active_this_frame = true;
        camera_state.snapshot_layer_index = Some(request.snapshot_layer_index);
    } else {
        // debug!("No snapshot requests for this frame");
        // No requests, ensure camera is inactive
        commands.entity(camera_entity).insert(Camera {
            is_active: false, // Deactivate
            ..default()       // Keep other settings, or fetch existing Camera component to modify
        });
        commands
            .entity(camera_entity)
            .remove::<ActiveSnapshotTarget>();
    }
}

/// Cleans up the snapshot camera after its use in a frame.
/// 在一帧中使用完毕后清理快照相机。
pub fn cleanup_snapshot_camera(
    mut commands: Commands,
    camera_state: Res<SnapshotCameraState>,
    // We could query for ActiveSnapshotTarget here to be more precise
) {
    if let Some(camera_entity) = camera_state.entity {
        if camera_state.is_active_this_frame {
            // Only if it was active
            // info!("Cleaning up snapshot camera, making it inactive.");
            // Make camera inactive for next frame unless prepare_snapshot_camera activates it
            commands.entity(camera_entity).insert(Camera {
                is_active: false,
                ..default() // Reset to default, or fetch existing to modify `is_active`
            });
            commands
                .entity(camera_entity)
                .remove::<ActiveSnapshotTarget>();
        }
    }
}

pub fn copy_temp_texture_to_snapshot_texture(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut camera_state: ResMut<SnapshotCameraState>,
    render_snapshot_temp_texture: Res<RenderSnapshotTempTexture>,
    render_snapshot_texture: Res<RenderSnapshotTexture>,
    gpu_images: Res<RenderAssets<GpuImage>>,
) {
    if let Some(layer_index) = camera_state.snapshot_layer_index {
        let mut command_encoder =
            render_device.create_command_encoder(&CommandEncoderDescriptor::default());

        let Some(snapshot_temp_image) = gpu_images.get(&render_snapshot_temp_texture.0) else {
            return;
        };

        let Some(snapshot_images) = gpu_images.get(&render_snapshot_texture.0) else {
            return;
        };

        command_encoder.copy_texture_to_texture(
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

        info!("Copied layer {} to snapshot texture", layer_index);

        render_queue.submit(std::iter::once(command_encoder.finish()));
        camera_state.snapshot_layer_index = None;
    }
}
