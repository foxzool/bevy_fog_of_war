use crate::{
    chunk::{InCameraView, MapChunk},
    fog::{FogMaterial, GpuChunks},
    vision::{GpuVisionParams, VisionParamsResource},
    vision_compute::{
        ChunkInfo, ChunkMeta, ChunkMetaBuffer, ExploredTexture, VisionComputeNodeLabel,
        VisionTexture,
    },
};
use bevy_app::prelude::*;
use bevy_asset::{AssetServer, Handle};
use bevy_color::LinearRgba;
use bevy_core_pipeline::{
    core_2d::graph::{Core2d, Node2d},
    fullscreen_vertex_shader::fullscreen_shader_vertex_state,
};
use bevy_ecs::{prelude::*, query::QueryItem, system::lifetimeless::Read};
use bevy_encase_derive::ShaderType;
use bevy_image::{BevyDefault, Image};
use bevy_log::{error, info};
use bevy_render::{
    RenderApp,
    diagnostic::RecordDiagnostics,
    extract_component::ExtractComponentPlugin,
    mesh::PrimitiveTopology,
    prelude::*,
    render_graph::{NodeRunError, RenderGraphApp, RenderGraphContext, ViewNode, ViewNodeRunner},
    render_resource::{
        BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, BlendComponent, BlendState,
        CachedRenderPipelineId, ColorTargetState, ColorWrites, CommandEncoder,
        DynamicUniformBuffer, Extent3d, FragmentState, FrontFace, LoadOp, MultisampleState,
        Operations, Origin3d, PipelineCache, PolygonMode, PrimitiveState,
        RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, ShaderStages,
        StorageTextureAccess, StoreOp, TexelCopyTextureInfo, TextureAspect, TextureFormat,
        binding_types::{storage_buffer_read_only, texture_storage_2d_array, uniform_buffer},
    },
    renderer::{RenderContext, RenderDevice, RenderQueue},
    view::{ExtractedView, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
};
use bevy_render_macros::{ExtractComponent, RenderLabel};
use bevy_time::Time;
use bevy_transform::prelude::GlobalTransform;
use bevy_utils::default;
use bytemuck::{Pod, Zeroable};

/// 区块纹理组件，存储区块的迷雾纹理 (Moved from chunk.rs)
/// Chunk texture component, stores the fog texture for a chunk (Moved from chunk.rs)
#[derive(Component, ExtractComponent, Debug, Clone, Default)]
pub struct ChunkTexture {
    /// Texture handle for the fog data
    pub explored: Handle<Image>,
}

/// 渲染插件，用于处理区块纹理提取
/// Render plugin for handling chunk texture extraction
pub struct ChunkRenderPlugin;

impl Plugin for ChunkRenderPlugin {
    fn build(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<FogOfWarMeta>()
                .add_plugins(ExtractComponentPlugin::<ChunkTexture>::default())
                .add_plugins(ExtractComponentPlugin::<InCameraView>::default())
                .add_plugins(ExtractComponentPlugin::<MapChunk>::default())
                .add_systems(ExtractSchedule, (prepare_fog_settings,));

            // 将迷雾节点放在 MainTransparentPass 和 EndMainPass 之间
            // Place fog node between MainTransparentPass and EndMainPass
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

pub fn prepare_fog_settings(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut fog_meta: ResMut<FogOfWarMeta>,
    views: Query<(Entity, &GlobalTransform, &FogMaterial), With<ExtractedView>>,
    time: Res<Time>,
) {
    let views_iter = views.iter();
    let view_count = views_iter.len();
    let Some(mut writer) =
        fog_meta
            .gpu_fog_settings
            .get_writer(view_count, &render_device, &render_queue)
    else {
        return;
    };
    for (entity, _transform, fog_settings) in views_iter {
        let settings = GpuFogMaterial {
            color: fog_settings.color.to_linear(),
            time: time.elapsed_secs(), // 使用当前时间 / Current time
        };

        commands.entity(entity).insert(ViewFogOfWarUniformOffset {
            offset: writer.write(&settings),
        });
    }
}

#[derive(Component)]
pub struct ViewFogOfWarUniformOffset {
    pub offset: u32,
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
        let shader = world.resource::<AssetServer>().load("shaders/fog2d.wgsl");

        let render_device = world.resource_mut::<RenderDevice>();

        // Combined Bind Group Layout (Group 0)
        let view_layout = render_device.create_bind_group_layout(
            "fog_of_war_combined_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    uniform_buffer::<ViewUniform>(true),                // Binding 0
                    uniform_buffer::<GpuFogMaterial>(true),             // Binding 1
                    storage_buffer_read_only::<GpuVisionParams>(false), // Binding 2
                    storage_buffer_read_only::<ChunkInfo>(false),       // Binding 3: Chunk info
                    texture_storage_2d_array(
                        TextureFormat::R8Unorm,
                        StorageTextureAccess::ReadOnly,
                    ), // 4
                    uniform_buffer::<ChunkMeta>(false),                 // 5
                    texture_storage_2d_array(
                        TextureFormat::Rgba8Unorm,
                        StorageTextureAccess::ReadOnly,
                    ), // 6
                    texture_storage_2d_array(
                        TextureFormat::Rgba8Unorm,
                        StorageTextureAccess::WriteOnly,
                    ), // 7
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
                            src_factor: bevy_render::render_resource::BlendFactor::SrcAlpha,
                            dst_factor: bevy_render::render_resource::BlendFactor::OneMinusSrcAlpha,
                            operation: bevy_render::render_resource::BlendOperation::Add,
                        },
                        alpha: BlendComponent {
                            src_factor: bevy_render::render_resource::BlendFactor::SrcAlpha,
                            dst_factor: bevy_render::render_resource::BlendFactor::OneMinusSrcAlpha,
                            operation: bevy_render::render_resource::BlendOperation::Add,
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
    type ViewQuery = (
        Read<ViewTarget>,
        Read<ViewFogOfWarUniformOffset>,
        Read<ViewUniformOffset>,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, view_fog_offset, view_uniform_offset): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let pipeline_cache = world.resource::<PipelineCache>();
        let fog_of_war_pipeline = world.resource::<FogOfWar2dPipeline>();
        let view_uniforms = world.resource::<ViewUniforms>();
        let fog_meta = world.resource::<FogOfWarMeta>();
        let vision_params_resource = world.resource::<VisionParamsResource>();
        let chunk_meta_buffer = world.resource::<ChunkMetaBuffer>();
        let explored_texture = world.resource::<ExploredTexture>();
        let vision_texture = world.resource::<VisionTexture>();

        let chunk_info_resource = world.get_resource::<GpuChunks>(); // Get chunk info resource

        let Some(pipeline) = pipeline_cache.get_render_pipeline(fog_of_war_pipeline.pipeline_id)
        else {
            return Ok(());
        };

        let Some(settings_binding) = fog_meta.gpu_fog_settings.binding() else {
            return Ok(());
        };
        let Some(view_uniforms_binding) = view_uniforms.uniforms.binding() else {
            return Ok(());
        };

        let vision_params_buffer_binding = vision_params_resource
            .buffer
            .as_ref()
            .map(|b| b.as_entire_binding());

        let Some(vision_binding) = vision_params_buffer_binding else {
            return Ok(()); // Or handle missing vision buffer as needed
        };

        // Return early if vision params buffer is needed but not available
        // Get the chunk info buffer binding
        let chunk_info_buffer_binding =
            chunk_info_resource.and_then(|r| r.buffer.as_ref().map(|b| b.as_entire_binding()));

        let Some(chunk_info_buffer_binding) = chunk_info_buffer_binding else {
            error!("ChunkInfoResource not found! Cannot bind chunk info buffer.");
            return Ok(());
        };

        let (Some(explored_read), Some(explored_write)) =
            (&explored_texture.read, &explored_texture.write)
        else {
            return Ok(());
        };

        let Some(vision_read) = &vision_texture.write else {
            return Ok(());
        };

        let Some(chunk_meta_binding) = chunk_meta_buffer
            .buffer
            .as_ref()
            .map(|b| b.binding())
            .flatten()
        else {
            return Ok(());
        };

        let view = view_target.main_texture_view();

        let view_bind_group = render_context.render_device().create_bind_group(
            "fog_combined_bind_group",        // Updated label
            &fog_of_war_pipeline.view_layout, // Use the combined layout
            &BindGroupEntries::sequential((
                view_uniforms_binding,        // Binding 0
                settings_binding.clone(),     // Binding 1
                vision_binding,               // Binding 2
                chunk_info_buffer_binding,    // Binding 3: Chunk info
                &vision_read.default_view,    // 4
                chunk_meta_binding,           // 5
                &explored_read.default_view,  // 6
                &explored_write.default_view, // 7
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
            &[view_uniform_offset.offset, view_fog_offset.offset], /* Dynamic offsets for view
                                                                    * and fog settings */
        );

        render_pass.draw(0..3, 0..1);
        pass_span.end(&mut render_pass);
        Ok(())
    }
}

// 清空 explored_texture 的某一层
pub fn clear_explored_texture_layer(
    render_device: &RenderDevice,
    encoder: &mut CommandEncoder,
    explored_texture: &ExploredTexture,
    layer_id: u32,
    width: u32,
    height: u32,
) {
    if let Some(write) = &explored_texture.write {
        // 构造一块全 0 的数据
        let zero_data = vec![0u8; (width * height) as usize];

        // 创建临时 buffer
        let buffer = render_device.create_buffer_with_data(
            &bevy_render::render_resource::BufferInitDescriptor {
                label: Some("clear_explored_layer_buffer"),
                contents: &zero_data,
                usage: bevy_render::render_resource::BufferUsages::COPY_SRC,
            },
        );

        // 拷贝 buffer 到纹理指定 z 层
        encoder.copy_buffer_to_texture(
            bevy_render::render_resource::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: bevy_render::render_resource::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width),
                    rows_per_image: Some(height),
                },
            },
            TexelCopyTextureInfo {
                texture: &write.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: layer_id,
                },
                aspect: TextureAspect::All,
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

// 复制 explored_texture 的某一层到另一层
pub fn copy_explored_texture_layer(
    encoder: &mut CommandEncoder,
    explored_texture: &ExploredTexture,
    from: u32,
    to: u32,
    width: u32,
    height: u32,
) {
    if let Some(write) = &explored_texture.write {
        println!("copy {from} to {to}");
        // Use passed encoder
        encoder.copy_texture_to_texture(
            TexelCopyTextureInfo {
                texture: &write.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: from,
                },
                aspect: TextureAspect::All,
            },
            TexelCopyTextureInfo {
                texture: &write.texture,
                mip_level: 0,
                origin: Origin3d { x: 0, y: 0, z: to },
                aspect: TextureAspect::All,
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// 迷雾设置的GPU表示
/// GPU representation of fog settings
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct GpuFogMaterial {
    color: LinearRgba,
    time: f32, // 当前时间 / Current time
}

#[derive(Default, Resource)]
pub struct FogOfWarMeta {
    pub gpu_fog_settings: DynamicUniformBuffer<GpuFogMaterial>,
}
