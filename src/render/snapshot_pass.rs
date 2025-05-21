// src/render/snapshot_pass.rs

use crate::components::{ActiveSnapshotTarget, SnapshotCamera};
use crate::render::extract::{
    RenderSnapshotTempTexture, RenderSnapshotTexture,
};
use crate::snapshot::SnapshotCameraState;
use bevy::math::FloatOrd;
use bevy::render::camera::{ImageRenderTarget, RenderTarget};
use bevy::render::render_resource::{
    BufferUsages, CommandEncoderDescriptor, Extent3d, Origin3d, StoreOp, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect, TextureFormat,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
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

        info!(
            "Copying temp texture to snapshot layer {}. Temp texture size: {}x{}, format: {:?}",
            layer_index,
            snapshot_temp_image.size.width,
            snapshot_temp_image.size.height,
            snapshot_temp_image.texture_format
        );

        render_queue.submit(std::iter::once(command_encoder.finish()));
        camera_state.snapshot_layer_index = None;
    }
}
