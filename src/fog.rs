use bevy::core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy::ecs::query::QueryItem;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_graph::{RenderGraphApp, RenderLabel, ViewNode, ViewNodeRunner};
use bevy::render::render_resource::{
    BufferInitDescriptor, CachedRenderPipelineId, LoadOp, Operations, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp,
};
use bevy::render::renderer::RenderContext;
use bevy::render::Render;
use bevy::{
    core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state,
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_graph::{
            NodeRunError, RenderGraphContext,
        },
        render_resource::{
            BindGroup, BindGroupEntry, BindGroupLayout
            , BindGroupLayoutEntry, BindingResource, BindingType,
            ColorTargetState, ColorWrites, FragmentState, MultisampleState, PipelineCache,
            PrimitiveState, RenderPipelineDescriptor, SamplerBindingType, SamplerDescriptor,
            ShaderStages, TextureFormat, TextureSampleType, TextureViewDimension,
        },
        render_resource::{Buffer, BufferUsages, Sampler, ShaderType},
        renderer::RenderDevice,
        view::{ExtractedView, ViewTarget},
        RenderApp, RenderSet,
    },
};
use bytemuck;

/// 迷雾相机标记组件
/// Fog camera marker component
#[derive(Component, ExtractComponent, Default, Clone, Copy, Debug)]
pub struct FogCameraMarker;

/// 迷雾设置资源
/// Fog settings resource
#[derive(Resource, Clone, ExtractResource)]
pub struct FogSettings {
    /// 迷雾颜色
    /// Fog color
    pub color: Color,
    /// 迷雾密度
    /// Fog density
    pub density: f32,
    /// 迷雾范围
    /// Fog range
    pub fog_range: f32,
    /// 迷雾最大强度
    /// Maximum fog intensity
    pub max_intensity: f32,
}

impl Default for FogSettings {
    fn default() -> Self {
        Self {
            color: Color::srgba(0.0, 0.0, 0.0, 1.0), // 黑色迷雾 / Black fog
            density: 0.05,
            fog_range: 1000.0,
            max_intensity: 0.8,
        }
    }
}

/// 迷雾插件
/// Fog plugin
pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FogSettings>()
            .add_plugins(ExtractComponentPlugin::<FogCameraMarker>::default())
            .add_plugins(ExtractResourcePlugin::<FogSettings>::default());

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .add_systems(Render, prepare_fog_settings.in_set(RenderSet::Prepare))
            .add_render_graph_node::<ViewNodeRunner<FogNode>>(Core2d, FogNodeLabel)
            .add_render_graph_edges(
                Core2d,
                (
                    Node2d::MainTransparentPass,
                    FogNodeLabel,
                    Node2d::EndMainPass,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.init_resource::<FogPipeline>();
    }
}

/// 迷雾节点名称
/// Fog node name
#[derive(RenderLabel, Debug, Clone, Hash, PartialEq, Eq)]
pub struct FogNodeLabel;

/// 迷雾管道
/// Fog pipeline
#[derive(Resource)]
struct FogPipeline {
    layout: BindGroupLayout,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for FogPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let pipeline_cache = world.resource::<PipelineCache>();

        let layout = render_device.create_bind_group_layout(
            "fog_bind_group_layout",
            &[
                // 屏幕纹理
                // Screen texture
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // 纹理采样器
                // Texture sampler
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                // 迷雾设置
                // Fog settings
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: bevy::render::render_resource::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        );

        let sampler = render_device.create_sampler(&SamplerDescriptor::default());

        let shader = world.resource::<AssetServer>().load("shaders/fog.wgsl");

        let pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("fog_pipeline".into()),
            layout: vec![layout.clone()],
            vertex: fullscreen_shader_vertex_state(),
            fragment: Some(FragmentState {
                shader,
                shader_defs: vec![],
                entry_point: "fragment".into(),
                // 使用与渲染通道相同的格式
                // Use the same format as the render pass
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::Bgra8UnormSrgb, // 显式指定格式而不是使用 bevy_default()
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            push_constant_ranges: vec![],
            zero_initialize_workgroup_memory: false,
        });

        Self {
            layout,
            sampler,
            pipeline_id,
        }
    }
}

/// 迷雾设置的GPU表示
/// GPU representation of fog settings
#[repr(C)]
#[derive(ShaderType, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuFogSettings {
    color: Vec4,
    density: f32,
    _padding1: f32,
    _padding2: f32,
    _padding3: f32,
    camera_position: Vec2,
    fog_range: f32,
    max_intensity: f32,
}

/// 准备迷雾设置
/// Prepare fog settings
#[derive(Resource)]
struct FogSettingsUniform {
    buffer: Buffer,
}

/// 准备迷雾设置系统
/// Prepare fog settings system
fn prepare_fog_settings(
    render_device: Res<RenderDevice>,
    fog_settings: Res<FogSettings>,
    mut commands: Commands,
    fog_settings_uniform: Option<ResMut<FogSettingsUniform>>,
    views: Query<(&ExtractedView, &Camera, &GlobalTransform), With<FogCameraMarker>>,
) {
    // 只处理第一个相机
    // Only process the first camera
    let (view, camera, transform) = match views.iter().next() {
        Some(v) => v,
        None => return,
    };
    let camera_position = transform.translation().truncate();

    let settings = GpuFogSettings {
        color: fog_settings.color.to_linear().to_vec4().into(),
        density: fog_settings.density,
        _padding1: 0.0,
        _padding2: 0.0,
        _padding3: 0.0,
        camera_position,
        fog_range: fog_settings.fog_range,
        max_intensity: fog_settings.max_intensity,
    };

    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("fog_settings_uniform_buffer"),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        contents: bytemuck::cast_slice(&[settings]),
    });

    if let Some(mut fog_settings_uniform) = fog_settings_uniform {
        *fog_settings_uniform = FogSettingsUniform { buffer };
    } else {
        commands.insert_resource(FogSettingsUniform { buffer });
    }
}

/// 迷雾节点
/// Fog node
struct FogNode {
    bind_group: Option<BindGroup>,
}

impl FogNode {
    fn new() -> Self {
        Self { bind_group: None }
    }
}

impl ViewNode for FogNode {
    type ViewQuery = (&'static ViewTarget,);

    fn update(&mut self, world: &mut World) {
        // 只处理带有FogCameraMarker组件的相机
        // Only process cameras with FogCameraMarker component
        let view_entity = {
            let mut query =
                world.query_filtered::<Entity, (With<ExtractedView>, With<FogCameraMarker>)>();
            let mut iter = query.iter(world);
            match iter.next() {
                Some(entity) => entity,
                None => return,
            }
        };

        // 先获取所有需要的资源
        // Get all required resources first
        let Some(view_target) = world.get::<ViewTarget>(view_entity) else {
            return;
        };

        let fog_pipeline = world.get_resource::<FogPipeline>();
        let fog_settings_uniform = world.get_resource::<FogSettingsUniform>();
        let render_device = world.resource::<RenderDevice>();

        // 如果任何必要资源不存在，则提前返回
        // Return early if any necessary resource doesn't exist
        let (fog_pipeline, fog_settings_uniform) = match (fog_pipeline, fog_settings_uniform) {
            (Some(pipeline), Some(settings)) => (pipeline, settings),
            _ => return,
        };

        self.bind_group = Some(render_device.create_bind_group(
            "fog_bind_group",
            &fog_pipeline.layout,
            &[
                // 使用 main_texture 作为输入资源
                // Use main_texture as input resource
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(view_target.main_texture_view()),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&fog_pipeline.sampler),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: fog_settings_uniform.buffer.as_entire_binding(),
                },
            ],
        ));
    }

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, ): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let fog_pipeline = world.resource::<FogPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();

        let bind_group = match &self.bind_group {
            Some(bind_group) => bind_group,
            None => return Ok(()),
        };

        let pipeline = match pipeline_cache.get_render_pipeline(fog_pipeline.pipeline_id) {
            Some(pipeline) => pipeline,
            None => return Ok(()),
        };
        // 使用 out_texture 作为渲染目标，而不是 main_texture
        // Use out_texture as render target instead of main_texture
        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("fog_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: view_target.out_texture(),
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_render_pipeline(pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        Ok(())
    }
}

impl FromWorld for FogNode {
    fn from_world(_world: &mut World) -> Self {
        Self::new()
    }
}
