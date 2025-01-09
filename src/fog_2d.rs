use bevy::ecs::component::ComponentHook;
use bevy::ecs::world::DeferredWorld;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex};
use bevy::render::render_resource::BufferBindingType;
use bevy::{
    app::{App, Plugin},
    asset::DirectAssetAccessExt,
    color::{Color, LinearRgba},
    core_pipeline::{
        core_2d::graph::{Core2d, Node2d},
        fullscreen_vertex_shader::fullscreen_shader_vertex_state,
    },
    ecs::query::QueryItem,
    image::BevyDefault,
    math::Vec4,
    prelude::{Camera, Component, FromWorld, Msaa, Resource, With, World},
    reflect::Reflect,
    render::{
        RenderApp,
        camera::ExtractedCamera,
        extract_component::{ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin},
        mesh::{PrimitiveTopology, VertexBufferLayout},
        render_graph::{
            NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner,
        },
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries,
            BindGroupLayoutEntry, BindingType, BlendComponent, BlendState, Buffer, BufferAddress,
            BufferInitDescriptor, BufferUsages, CachedRenderPipelineId, ColorTargetState,
            ColorWrites, Face, FragmentState, FrontFace, IndexFormat, LoadOp, MultisampleState,
            Operations, PipelineCache, PipelineLayoutDescriptor, PolygonMode, PrimitiveState,
            RawVertexBufferLayout, RawVertexState, RenderPassColorAttachment, RenderPassDescriptor,
            RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages,
            ShaderType, StoreOp, TextureFormat, TextureSampleType, TextureViewDimension,
            VertexAttribute, VertexFormat, VertexState, VertexStepMode,
            binding_types::{sampler, texture_2d, uniform_buffer},
        },
        renderer::{RenderContext, RenderDevice},
        view::ViewTarget,
    },
    utils::default,
};

pub struct FogOfWar2dPlugin;

impl Plugin for FogOfWar2dPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<FogOfWarSettings>().add_plugins((
            ExtractComponentPlugin::<FogOfWarSettings>::default(),
            UniformComponentPlugin::<FogOfWarSettings>::default(),
        ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .add_render_graph_node::<ViewNodeRunner<FogOfWar2dNode>>(Core2d, FogOfWarLabel)
            .add_render_graph_edges(
                Core2d,
                (Node2d::MainOpaquePass, FogOfWarLabel, Node2d::EndMainPass),
            );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            // Initialize the pipeline
            .init_resource::<FogOfWar2dPipeline>();
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct FogOfWarLabel;

#[derive(Component, Debug, Clone, Reflect, ExtractComponent, ShaderType)]
pub struct FogOfWarSettings {
    pub fog_color: LinearRgba,
}

impl Default for FogOfWarSettings {
    fn default() -> Self {
        Self {
            fog_color: Color::BLACK.into(),
        }
    }
}

#[derive(Default)]
struct FogOfWar2dNode;

impl ViewNode for FogOfWar2dNode {
    type ViewQuery = (
        &'static ViewTarget,
        &'static FogOfWarSettings,
        &'static DynamicUniformIndex<FogOfWarSettings>,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, _fog_of_war_settings, settings_index): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let fog_of_war_pipeline = world.resource::<FogOfWar2dPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();

        let Some(pipeline) = pipeline_cache.get_render_pipeline(fog_of_war_pipeline.pipeline_id)
        else {
            println!("Failed to get pipeline");
            return Ok(());
        };

        let settings_uniforms = world.resource::<ComponentUniforms<FogOfWarSettings>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
            return Ok(());
        };

        let view = view_target.main_texture_view();

        let bind_group = render_context.render_device().create_bind_group(
            "post_process_bind_group",
            &fog_of_war_pipeline.bind_group_layout,
            // It's important for this to match the BindGroupLayout defined in the PostProcessPipeline
            &BindGroupEntries::sequential((
                // // Make sure to use the source view
                // view,
                // // // Use the sampler created for the pipeline
                // &fog_of_war_pipeline.sampler,
                // Set the settings binding
                settings_binding.clone(),
            )),
        );

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

        render_pass.set_render_pipeline(pipeline);
        render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
        render_pass.set_vertex_buffer(0, fog_of_war_pipeline.vertex_buffer.slice(..));
        render_pass.set_index_buffer(
            fog_of_war_pipeline.index_buffer.slice(..),
            0,
            IndexFormat::Uint16,
        );

        render_pass.draw_indexed(0..6, 0, 0..1);

        Ok(())
    }
}

#[derive(Resource)]
struct FogOfWar2dPipeline {
    bind_group_layout: BindGroupLayout,
    pipeline_id: CachedRenderPipelineId,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
}

impl FromWorld for FogOfWar2dPipeline {
    fn from_world(world: &mut World) -> Self {
        let shader = world.load_asset(SHADER_ASSET_PATH);
        let render_device = world.resource_mut::<RenderDevice>();

        let bind_group_layout = render_device.create_bind_group_layout(
            "fog_of_war_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (uniform_buffer::<FogOfWarSettings>(true),),
            ),
        );

        let vertex_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: BufferUsages::VERTEX,
        });
        let index_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: BufferUsages::INDEX,
        });

        let pipeline_id = world.resource_mut::<PipelineCache>().queue_render_pipeline(
            RenderPipelineDescriptor {
                label: Some("fog_of_war_2d_pipeline".into()),
                layout: vec![bind_group_layout.clone()],
                vertex: VertexState {
                    shader: shader.clone_weak(),
                    entry_point: "vs_main".into(),
                    buffers: vec![Vertex::desc()],
                    shader_defs: vec![],
                },
                fragment: Some(FragmentState {
                    shader,
                    shader_defs: vec![],
                    entry_point: "fs_main".into(),
                    targets: vec![Some(ColorTargetState {
                        format: TextureFormat::Rgba8UnormSrgb, // 明确指定格式
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
            },
        );

        Self {
            bind_group_layout,
            pipeline_id,
            vertex_buffer,
            index_buffer,
        }
    }
}

const SHADER_ASSET_PATH: &str = "shaders/fog_of_war.wgsl";
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
}

impl Vertex {
    fn desc() -> VertexBufferLayout {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: vec![
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x4,
                },
            ],
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-1.0, -1.0, 0.0], // 左下
        color: [0.0, 0.0, 0.0, 1.0],
    },
    Vertex {
        position: [1.0, -1.0, 0.0], // 右下
        color: [0.0, 0.0, 0.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0, 0.0], // 右上
        color: [0.0, 0.0, 0.0, 1.0],
    },
    Vertex {
        position: [-1.0, 1.0, 0.0], // 左上
        color: [0.0, 0.0, 0.0, 1.0],
    },
];

const INDICES: &[u16] = &[0, 1, 2, 0, 2, 3]; // 两个三角形组成一个矩形
