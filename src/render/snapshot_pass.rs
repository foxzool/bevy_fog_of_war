// src/render/snapshot_pass.rs

use crate::components::{ActiveSnapshotTarget, SnapshotCamera}; // SNAPSHOT_RENDER_LAYER_ID
use crate::render::extract::{RenderSnapshotTexture, SnapshotRequestQueue};
use bevy::core_pipeline::core_2d::AlphaMask2d;
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::lifetimeless::Read;
use bevy::math::FloatOrd;
use bevy::render::camera::{ImageRenderTarget, RenderTarget};
use bevy::render::render_phase::ViewBinnedRenderPhases;
use bevy::render::render_resource::StoreOp;
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
use std::num::NonZeroU32;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct SnapshotNodeLabel;

/// Marker component for entities in the RenderWorld that should be part of snapshots.
/// RenderWorld 中应包含在快照中的实体的标记组件。
#[derive(Component, Clone, Copy)]
pub struct RenderWorldSnapshotVisible;

/// Resource to manage the state of the snapshot camera entity in the RenderWorld.
/// 用于管理 RenderWorld 中快照相机实体状态的资源。
#[derive(Resource, Default)]
pub struct SnapshotCameraState {
    pub entity: Option<Entity>,
    pub is_active_this_frame: bool, // Was it activated for the current frame's snapshot?
}

/// Prepares and configures the single SnapshotCamera entity for the current frame's snapshot request (if any).
/// 为当前帧的快照请求（如果有）准备和配置单个 SnapshotCamera 实体。
pub fn prepare_snapshot_camera(
    mut commands: Commands,
    mut camera_state: ResMut<SnapshotCameraState>,
    mut snapshot_requests: ResMut<SnapshotRequestQueue>, // Extracted requests
    // Query to find the existing SnapshotCamera entity
    snapshot_camera_query: Query<Entity, With<SnapshotCamera>>,
    render_snapshot_texture: Res<RenderSnapshotTexture>, // To set the nominal target
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
    } else {
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

pub struct SnapshotNode {
    query: QueryState<
        (
            Read<ViewUniformOffset>,
            Read<ExtractedView>,
            Read<ActiveSnapshotTarget>,
            // Read<ViewTarget>,
        ),
        // With<SnapshotCamera>,
    >,
}

impl FromWorld for SnapshotNode {
    fn from_world(world: &mut World) -> Self {
        SnapshotNode {
            query: QueryState::new(world),
        }
    }
}

impl ViewNode for SnapshotNode {
    // We need to query for the camera entity that has our ActiveSnapshotTarget
    // and its associated render phases.
    type ViewQuery = ();

    fn update(&mut self, world: &mut World) {
        self.query.update_archetypes(world);
        

        for (view_uniform_offset, view, active_target) in self.query.iter_manual(world) {
            println!("active_target: {}", active_target.snapshot_layer_index);
        }
    }

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let (Some(opaque_phases), Some(alpha_mask_phases)) = (
            world.get_resource::<ViewBinnedRenderPhases<Opaque2d>>(),
            world.get_resource::<ViewBinnedRenderPhases<AlphaMask2d>>(),
        ) else {
            return Ok(());
        };
        let Some(camera_state) = world.get_resource::<SnapshotCameraState>() else {
            return Ok(());
        };

        let Some(view_entity) = camera_state.entity else {
            return Ok(());
        };

        debug!("SnapshotNode running for view entity: {}", view_entity);
        let Ok((view_uniform_offset, view, active_target)) =
            self.query.get_manual(world, view_entity)
        else {
            return Ok(());
        };
        debug!(
            "SnapshotNode running for layer: {}",
            active_target.snapshot_layer_index
        );

        let (Some(opaque_phase), Some(alpha_mask_phase)) = (
            opaque_phases.get(&view.retained_view_entity),
            alpha_mask_phases.get(&view.retained_view_entity),
        ) else {
            return Ok(());
        };
        info!(
            "SnapshotNode running for layer: {}",
            active_target.snapshot_layer_index
        );

        let snapshot_texture_array_handle = world.resource::<RenderSnapshotTexture>();
        let gpu_images = world.resource::<RenderAssets<GpuImage>>();

        let Some(snapshot_gpu_image) = gpu_images.get(&snapshot_texture_array_handle.0) else {
            error!("SnapshotNode: SnapshotTextureArray GpuImage not found.");
            return Ok(());
        };

        let label = format!("snapshot_layer_{}", active_target.snapshot_layer_index);
        println!(
            "SnapshotNode: Creating view for layer {}",
            active_target.snapshot_layer_index
        );
        // Create a texture view for the specific layer
        let layer_view_descriptor = TextureViewDescriptor {
            label: Some(&label),
            format: Some(snapshot_gpu_image.texture_format),
            dimension: Some(bevy::render::render_resource::TextureViewDimension::D2),
            usage: None,
            aspect: bevy::render::render_resource::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: Some(1),
            base_array_layer: active_target.snapshot_layer_index,
            array_layer_count: Some(1),
        };
        let target_layer_view = snapshot_gpu_image
            .texture
            .create_view(&layer_view_descriptor);

        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("snapshot_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_layer_view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(default()), // Clear to transparent
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None, // No depth for 2D snapshots usually
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        //
        // let view_uniforms = world.resource::<ViewUniforms>();
        // let view_uniforms_resource = world.resource::<ViewUniforms>();
        // let view_bind_group = render_context.render_device().create_bind_group(
        //     "snapshot_node_view_bind_group",
        //     &view_uniforms_resource.layout,    // 使用 ViewUniforms 自带的布局
        //     &BindGroupEntries::single(view_uniform_binding), // 创建 BindGroupEntries
        // );
        // render_pass.set_bind_group(0, &view_bind_group.value, &[view_uniform_offset.offset]);

        if !opaque_phase.is_empty() {
            // trace!("SnapshotNode: Rendering {} opaque items for layer {}", opaque_phase.items.len(), active_target.snapshot_layer_index);
            let _ = opaque_phase.render(&mut render_pass, world, view_entity);
        }
        if !alpha_mask_phase.is_empty() {
            // trace!("SnapshotNode: Rendering {} transparent items for layer {}", transparent_phase.items.len(), active_target.snapshot_layer_index);
            let _ = alpha_mask_phase.render(&mut render_pass, world, view_entity);
        }

        Ok(())
    }
}
