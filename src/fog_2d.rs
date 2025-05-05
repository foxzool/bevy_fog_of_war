use crate::chunk::{FogSettingsBuffer, FogSettingsUniform, GpuChunks};
use crate::vision::{GpuVisionParams, VisionParamsResource};
use crate::{
    chunk::{InCameraView, FogChunk},
    vision::{ChunkInfo, ExploredTexture, VisionComputeNodeLabel, VisionTexture},
};
use bevy::core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::lifetimeless::Read;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::ExtractComponentPlugin;
use bevy::render::render_graph::{
    NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner,
};
use bevy::render::render_resource::binding_types::{
    storage_buffer_read_only, texture_storage_2d_array, uniform_buffer,
};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, BlendComponent, BlendState,
    CachedRenderPipelineId, ColorTargetState, ColorWrites, FragmentState, FrontFace, LoadOp,
    MultisampleState, Operations, PipelineCache, PolygonMode, PrimitiveState, PrimitiveTopology,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, ShaderStages,
    StorageTextureAccess, StoreOp, TextureFormat,
};
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::view::{ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms};

const SHADER_ASSET_PATH: &str = "shaders/fog_2d.wgsl";

/// 渲染插件，用于处理区块纹理提取
/// Render plugin for handling chunk texture extraction
pub struct Fog2DRenderPlugin;

impl Plugin for Fog2DRenderPlugin {
    fn build(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .add_plugins(ExtractComponentPlugin::<InCameraView>::default())
                .add_plugins(ExtractComponentPlugin::<FogChunk>::default());

            render_app
                .add_render_graph_node::<ViewNodeRunner<FogNode2d>>(Core2d, FogNode2dLabel)
                .add_render_graph_edges(
                    Core2d,
                    (VisionComputeNodeLabel, FogNode2dLabel, Node2d::EndMainPass),
                );
        }
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app.init_resource::<FogOfWar2dPipeline>();
    }
}

/// 迷雾节点名称
/// Fog node name
#[derive(RenderLabel, Debug, Clone, Hash, PartialEq, Eq)]
pub struct FogNode2dLabel;

#[derive(Resource)]
pub struct FogOfWar2dPipeline {
    pub view_layout: BindGroupLayout,
    pub pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for FogOfWar2dPipeline {
    fn from_world(world: &mut World) -> Self {
        let shader = world.resource::<AssetServer>().load(SHADER_ASSET_PATH);

        let render_device = world.resource_mut::<RenderDevice>();

        // Combined Bind Group Layout (Group 0)
        let view_layout = render_device.create_bind_group_layout(
            "fog_of_war_combined_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    uniform_buffer::<ViewUniform>(true),                // Binding 0
                    uniform_buffer::<FogSettingsUniform>(false),        // Binding 1
                    storage_buffer_read_only::<GpuVisionParams>(false), // Binding 2
                    storage_buffer_read_only::<ChunkInfo>(false),       // Binding 3: Chunk info
                    texture_storage_2d_array(
                        TextureFormat::R8Unorm,
                        StorageTextureAccess::ReadOnly,
                    ), // 4
                    texture_storage_2d_array(
                        TextureFormat::Rgba8Unorm,
                        StorageTextureAccess::ReadOnly,
                    ), // 5
                ),
            ),
        );

        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("fog_of_war_2d_pipeline".into()),
            layout: vec![view_layout.clone()], // Use combined layout only
            vertex: fullscreen_shader_vertex_state(),
            fragment: Some(FragmentState {
                shader: shader.clone(),
                shader_defs: vec![],
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::bevy_default(),
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: bevy::render::render_resource::BlendFactor::SrcAlpha,
                            dst_factor:
                                bevy::render::render_resource::BlendFactor::OneMinusSrcAlpha,
                            operation: bevy::render::render_resource::BlendOperation::Add,
                        },
                        alpha: BlendComponent {
                            src_factor: bevy::render::render_resource::BlendFactor::SrcAlpha,
                            dst_factor:
                                bevy::render::render_resource::BlendFactor::OneMinusSrcAlpha,
                            operation: bevy::render::render_resource::BlendOperation::Add,
                        },
                    }),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            push_constant_ranges: vec![],
            zero_initialize_workgroup_memory: false,
        });

        Self {
            view_layout,
            pipeline_id,
        }
    }
}

#[derive(Default)]
pub struct FogNode2d;

impl ViewNode for FogNode2d {
    type ViewQuery = (Read<ViewTarget>, Read<ViewUniformOffset>);

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, view_uniform_offset): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let pipeline_cache = world.resource::<PipelineCache>();
        let fog_of_war_pipeline = world.resource::<FogOfWar2dPipeline>();
        let view_uniforms = world.resource::<ViewUniforms>();
        let vision_params_resource = world.resource::<VisionParamsResource>();
        let fog_settings_buffer = world.resource::<FogSettingsBuffer>();
        let explored_texture = world.resource::<ExploredTexture>();
        // let snapshot_texture = world.resource::<SnapshotTexture>();

        let vision_texture = world.resource::<VisionTexture>();

        let chunk_info_resource = world.get_resource::<GpuChunks>(); // Get chunk info resource

        let Some(pipeline) = pipeline_cache.get_render_pipeline(fog_of_war_pipeline.pipeline_id)
        else {
            return Ok(());
        };

        let Some(view_uniforms_binding) = view_uniforms.uniforms.binding() else {
            return Ok(());
        };

        let vision_buffer_binding = vision_params_resource.buffer.as_entire_binding();

        // Return early if vision params buffer is needed but not available
        // Get the chunk info buffer binding
        let chunk_info_buffer_binding =
            chunk_info_resource.and_then(|r| r.buffer.as_ref().map(|b| b.as_entire_binding()));

        let Some(chunk_info_buffer_binding) = chunk_info_buffer_binding else {
            error!("ChunkInfoResource not found! Cannot bind chunk info buffer.");
            return Ok(());
        };

        let (Some(explored_read), Some(_explored_write)) =
            (&explored_texture.read, &explored_texture.write)
        else {
            return Ok(());
        };

        let Some(vision_read) = &vision_texture.write else {
            return Ok(());
        };

        let view = view_target.main_texture_view();

        let view_bind_group = render_context.render_device().create_bind_group(
            "fog_combined_bind_group",        // Updated label
            &fog_of_war_pipeline.view_layout, // Use the combined layout
            &BindGroupEntries::sequential((
                view_uniforms_binding,                          // Binding 0
                fog_settings_buffer.buffer.as_entire_binding(), // Binding 1
                vision_buffer_binding,                          // Binding 2
                chunk_info_buffer_binding,                      // Binding 3: Chunk info
                &vision_read.default_view,                      // 4
                &explored_read.default_view,                    // 5
            )),
        );

        let diagnostics = render_context.diagnostic_recorder();

        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("fog_of_war_2d_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            ..default()
        });

        let pass_span = diagnostics.pass_span(&mut render_pass, "fog_of_war_2d_pass");

        render_pass.set_render_pipeline(pipeline);
        render_pass.set_bind_group(
            0,
            &view_bind_group,
            &[view_uniform_offset.offset], /* Dynamic offsets for view
                                            * and fog settings */
        );

        render_pass.draw(0..3, 0..1);
        pass_span.end(&mut render_pass);
        Ok(())
    }
}
