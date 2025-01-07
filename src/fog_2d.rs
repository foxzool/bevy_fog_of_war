use bevy::app::{App, Plugin};
use bevy::asset::DirectAssetAccessExt;
use bevy::color::{Color, LinearRgba};
use bevy::core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::ecs::query::QueryItem;
use bevy::image::BevyDefault;
use bevy::math::Vec4;
use bevy::prelude::{Camera, Component, FromWorld, Msaa, Resource, With, World};
use bevy::reflect::Reflect;
use bevy::render::RenderApp;
use bevy::render::camera::ExtractedCamera;
use bevy::render::extract_component::{
    ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin,
};
use bevy::render::mesh::{PrimitiveTopology, VertexBufferLayout};
use bevy::render::render_graph::{
    NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner,
};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, uniform_buffer};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, BindGroupLayoutEntry, BindingType,
    BlendState, Buffer, BufferAddress, BufferInitDescriptor, BufferUsages, CachedRenderPipelineId,
    ColorTargetState, ColorWrites, Face, FragmentState, FrontFace, IndexFormat, LoadOp,
    MultisampleState, Operations, PipelineCache, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, RawVertexBufferLayout, RawVertexState, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderStages, ShaderType, StoreOp, TextureFormat, TextureSampleType, TextureViewDimension,
    VertexAttribute, VertexFormat, VertexState, VertexStepMode,
};
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::view::ViewTarget;

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
            .add_render_graph_edges(Core2d, (Node2d::EndMainPassPostProcessing, FogOfWarLabel));
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
    pub fog_start: f32,
    pub fog_end: f32,
}

impl Default for FogOfWarSettings {
    fn default() -> Self {
        Self {
            fog_color: Vec4::new(0.0, 0.0, 0.0, 1.0),
            fog_start: 100.0,
            fog_end: 50.0,
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
        let fog_of_war_2d_pipeline = world.resource::<FogOfWar2dPipeline>();

        let pipeline_cache = world.resource::<PipelineCache>();

        // Get the pipeline from the cache
        let Some(pipeline) = pipeline_cache.get_render_pipeline(fog_of_war_2d_pipeline.pipeline_id)
        else {
            return Ok(());
        };

        let post_process = target.post_process_write();
        let color_attachments = [Some(target.get_color_attachment())];


        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("fog_of_war_2d_pass"),
            color_attachments: &[
                Some(RenderPassColorAttachment {
                    view: &post_process.destination,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(LinearRgba::BLACK.into()),
                        store: StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        if let Some(viewport) = camera.viewport.as_ref() {
            render_pass.set_camera_viewport(viewport);
        }

        render_pass.set_render_pipeline(pipeline);
        // render_pass.set_bind_group(0, &bind_group, &[]);
        // render_pass.set_vertex_buffer(0, fog_of_war_2d_pipeline.vertex_buffer.slice(..));
        // render_pass.set_index_buffer(
        //     fog_of_war_2d_pipeline.index_buffer.slice(..),
        //     0,
        //     IndexFormat::Uint16,
        // );
        // render_pass.draw_indexed(0..(INDICES.len() as u32), 0, 0..1);
        render_pass.draw(0..3, 0..1);
        Ok(())
    }
}

#[derive(Resource)]
struct FogOfWar2dPipeline {
    // layout: BindGroupLayout,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
    // vertex_buffer: Buffer,
    // index_buffer: Buffer,
}

impl FromWorld for FogOfWar2dPipeline {
    fn from_world(world: &mut World) -> Self {
        let mut query = world.query_filtered::<&Msaa, (With<Camera>, With<FogOfWarSettings>)>();
        let msaa = match query.get_single(world) {
            Ok(m) => *m,
            Err(_) => Msaa::Sample4,
        };
        let render_device = world.resource::<RenderDevice>();

        // We can create the sampler here since it won't change at runtime and doesn't depend on the view
        let sampler = render_device.create_sampler(&SamplerDescriptor::default());

        // Get the shader handle
        let shader = world.load_asset(SHADER_ASSET_PATH);

        let pipeline_id = world
            .resource_mut::<PipelineCache>()
            // This will add the pipeline to the cache and queue its creation
            .queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("fog_of_war_2d_pipeline".into()),
                layout: vec![],
                // This will setup a fullscreen triangle for the vertex state
                vertex: VertexState {
                    shader: shader.clone_weak(),
                    entry_point: "vs_main".into(),
                    buffers: vec![],
                    shader_defs: vec![],
                },
                fragment: Some(FragmentState {
                    shader,
                    shader_defs: vec![],
                    // Make sure this matches the entry point of your shader.
                    // It can be anything as long as it matches here and in the shader.
                    entry_point: "fs_main".into(),
                    targets: vec![Some(ColorTargetState {
                        format: TextureFormat::bevy_default(),
                        blend: Some(BlendState::REPLACE),
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                // All the following properties are not important for this effect so just use the default values.
                // This struct doesn't have the Default trait implemented because not all fields can have a default value.
                primitive: PrimitiveState {
                    topology: PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: FrontFace::Ccw,
                    cull_mode: Some(Face::Back),
                    unclipped_depth: false,
                    polygon_mode: PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: MultisampleState::default(),
                push_constant_ranges: vec![],
                zero_initialize_workgroup_memory: false,
            });

        Self {
            // layout,
            sampler,
            pipeline_id,
        }
    }
}

const SHADER_ASSET_PATH: &str = "shaders/fog_of_war.wgsl";

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

impl Vertex {
    fn desc() -> VertexBufferLayout {
        use std::mem;
        VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: vec![
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: mem::size_of::<[f32; 3]>() as BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x2,
                },
            ],
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-1.0, -1.0, 0.0],
        tex_coords: [0.0, 1.0],
    }, // A
    Vertex {
        position: [1.0, -1.0, 0.0],
        tex_coords: [1.0, 1.0],
    }, // B
    Vertex {
        position: [-1.0, 1.0, 0.0],
        tex_coords: [0.0, 0.0],
    }, // C
    Vertex {
        position: [1.0, 1.0, 0.0],
        tex_coords: [1.0, 0.0],
    }, // d
];

const INDICES: &[u16] = &[0, 1, 2, 2, 1, 3];
