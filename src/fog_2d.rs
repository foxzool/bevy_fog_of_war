use std::collections::BTreeMap;
use bevy::core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy::ecs::query::QueryItem;
use bevy::prelude::*;
use bevy::render::extract_component::{
    ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
    UniformComponentPlugin,
};
use bevy::render::mesh::{PrimitiveTopology, VertexBufferLayout};
use bevy::render::render_graph::{
    NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner,
};
use bevy::render::render_resource::binding_types::{
    storage_buffer_read_only_sized, uniform_buffer,
};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, BlendComponent, BlendState, Buffer,
    BufferAddress, BufferInitDescriptor, BufferUsages, CachedRenderPipelineId, ColorTargetState,
    ColorWrites, FragmentState, FrontFace, IndexFormat, LoadOp, MultisampleState, Operations,
    PipelineCache, PolygonMode, PrimitiveState, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipelineDescriptor, ShaderStages, ShaderType, StorageBuffer, StoreOp, TextureFormat,
    VertexAttribute, VertexFormat, VertexState, VertexStepMode,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use bevy::render::view::ViewTarget;
use bevy::render::{Extract, Render, RenderApp, RenderSet};
use bevy::utils::{Entry, HashMap};
use bytemuck::Pod;
use bytemuck::Zeroable;


pub struct FogOfWar2dPlugin;

impl Plugin for FogOfWar2dPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<FogOfWarSettings>().add_plugins((
            ExtractComponentPlugin::<FogOfWarSettings>::default(),
            UniformComponentPlugin::<FogOfWarSettings>::default(),
        ));

        app.register_type::<FogSight2D>()
            .add_plugins((ExtractComponentPlugin::<FogSight2D>::default(),));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<FogSight2dBuffers>()
            .add_systems(ExtractSchedule, extract_buffers)
            .add_systems(Render, (prepare_buffers.in_set(RenderSet::Prepare),))
            .add_render_graph_node::<ViewNodeRunner<FogOfWar2dNode>>(Core2d, FogOfWarLabel)
            .add_render_graph_edges(
                Core2d,
                (
                    Node2d::MainTransparentPass,
                    FogOfWarLabel,
                    Node2d::EndMainPass,
                ),
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
    pub screen_size: Vec2,
}

impl Default for FogOfWarSettings {
    fn default() -> Self {
        Self {
            fog_color: Color::BLACK.into(),
            screen_size: Vec2::new(1280.0, 720.0),
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
        let fog_sight_buffers = world.resource::<FogSight2dBuffers>();

        let Some(pipeline) = pipeline_cache.get_render_pipeline(fog_of_war_pipeline.pipeline_id)
        else {
            return Ok(());
        };

        let settings_uniforms = world.resource::<ComponentUniforms<FogOfWarSettings>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
            return Ok(());
        };

        let view = view_target.main_texture_view();



        let bind_group = render_context.render_device().create_bind_group(
            None,
            &fog_of_war_pipeline.bind_group_layout,
            &BindGroupEntries::sequential((
                settings_binding.clone(),
                // sight_buffer.binding().unwrap(),
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
                (
                    uniform_buffer::<FogOfWarSettings>(true),
                    // storage_buffer_read_only_sized(false, None),
                ),
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

#[derive(Component, Debug, Copy, Clone, Reflect, ExtractComponent, ShaderType, Pod, Zeroable)]
#[repr(C)]
pub struct FogSight2D {
    pub position: Vec2,
    pub inner_radius: f32,
    pub outer_radius: f32,
}

impl Default for FogSight2D {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            inner_radius: 0.3,
            outer_radius: 0.5,
        }
    }
}

#[derive(Resource)]
pub(super) struct ExtractedSight2DBuffers {
    changed: Vec<(Entity, FogSight2D)>,
    removed: Vec<Entity>,
}

pub(super) fn extract_buffers(
    mut commands: Commands,
    changed: Extract<Query<(Entity, &FogSight2D), Changed<FogSight2D>>>,
    mut removed: Extract<RemovedComponents<FogSight2D>>,
) {
    commands.insert_resource(ExtractedSight2DBuffers {
        changed: changed
            .iter()
            .map(|(entity, settings)| (entity, settings.clone()))
            .collect(),
        removed: removed.read().collect(),
    });
}

#[derive(Resource, Default)]
pub(super) struct FogSight2dBuffers {
    pub(super) buffers: HashMap<Entity, StorageBuffer<FogSight2D>>,
}

pub(super) fn prepare_buffers(
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut extracted: ResMut<ExtractedSight2DBuffers>,
    mut buffers: ResMut<FogSight2dBuffers>,
) {
    for (entity, fog_sight_2d) in extracted.changed.drain(..) {
        match buffers.buffers.entry(entity) {
            Entry::Occupied(mut entry) => {
                let value = entry.get_mut();
                value.set(fog_sight_2d);
                value.write_buffer(&device, &queue);
            }
            Entry::Vacant(entry) => {
                let value = entry.insert(StorageBuffer::from(fog_sight_2d));

                value.write_buffer(&device, &queue);
            }
        }
    }

    for entity in extracted.removed.drain(..) {
        buffers.buffers.remove(&entity);
    }
}
