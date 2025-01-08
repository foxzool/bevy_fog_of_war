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
    pub fog_color: Vec4,
}

impl Default for FogOfWarSettings {
    fn default() -> Self {
        Self {
            fog_color: Vec4::new(0.0, 0.0, 0.0, 1.0),
        }
    }
}

#[derive(Default)]
struct FogOfWar2dNode {}

impl ViewNode for FogOfWar2dNode {
    type ViewQuery = (&'static ExtractedCamera, &'static ViewTarget);

    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (camera, target): QueryItem<'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        println!("Fog of War render pass is running");

        let pipeline_res = world.resource::<FogOfWar2dPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();

        let Some(pipeline) = pipeline_cache.get_render_pipeline(pipeline_res.pipeline_id) else {
            println!("Failed to get pipeline");
            return Ok(());
        };

        let view = target.main_texture_view();
        // println!("View format: {:?}", view.format());

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
        render_pass.set_bind_group(0, &pipeline_res.bind_group, &[]);
        render_pass.set_vertex_buffer(0, pipeline_res.vertex_buffer.slice(..));
        render_pass.set_index_buffer(pipeline_res.index_buffer.slice(..), 0, IndexFormat::Uint16);

        println!("Drawing fog of war with {} indices", INDICES.len());
        render_pass.draw_indexed(0..6, 0, 0..1);

        Ok(())
    }
}

#[derive(Resource)]
struct FogOfWar2dPipeline {
    bind_group: BindGroup,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    num_indices: u32,
}

impl FromWorld for FogOfWar2dPipeline {
    fn from_world(world: &mut World) -> Self {
        let shader = world.load_asset(SHADER_ASSET_PATH);
        let render_device = world.resource_mut::<RenderDevice>();
        let sampler = render_device.create_sampler(&SamplerDescriptor::default());

        // 创建 bind group layout
        let bind_group_layout = render_device.create_bind_group_layout("fog_of_war_layout", &[]);

        // 创建 bind group
        let bind_group =
            render_device.create_bind_group("fog_of_war_bind_group", &bind_group_layout, &[]);

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
                layout: vec![bind_group_layout],
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
            bind_group,
            sampler,
            pipeline_id,
            vertex_buffer,
            index_buffer,
            num_indices: INDICES.len() as u32,
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
