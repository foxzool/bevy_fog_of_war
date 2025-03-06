use bevy::core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy::ecs::query::QueryItem;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_graph::{RenderGraphApp, RenderLabel, ViewNode, ViewNodeRunner};
use bevy::render::render_resource::{AddressMode, BindGroupEntry, BindGroupLayoutEntry, BindingResource, BindingType, BlendComponent, BlendFactor, BlendOperation, BlendState, BufferBinding, BufferBindingType, BufferInitDescriptor, BufferSize, CachedRenderPipelineId, FilterMode, LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, SamplerBindingType, ShaderStages, StoreOp, TextureSampleType, TextureViewDimension};
use bevy::render::renderer::RenderContext;
use bevy::render::Render;
use bevy::{
    core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state,
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_graph::{NodeRunError, RenderGraphContext},
        render_resource::{
            BindGroup, BindGroupLayout, ColorTargetState, ColorWrites, FragmentState,
            MultisampleState, PipelineCache, PrimitiveState, RenderPipelineDescriptor,
            SamplerDescriptor, TextureFormat,
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
    /// 相机周围的透明区域半径
    /// Clear radius around camera
    pub clear_radius: f32,
    /// 边缘过渡效果宽度
    /// Edge falloff width
    pub clear_falloff: f32,
}

impl Default for FogSettings {
    fn default() -> Self {
        Self {
            color: Color::srgba(0.0, 0.0, 0.0, 1.0), // 黑色迷雾 / Black fog
            density: 0.05,
            fog_range: 1000.0,
            max_intensity: 0.8,
            clear_radius: 0.3,      // 默认相机周围透明区域半径 / Default clear radius
            clear_falloff: 0.1,      // 默认边缘过渡宽度 / Default edge falloff width
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

        // 将迷雾节点放在 MainTransparentPass 和 EndMainPass 之间
        // Place fog node between MainTransparentPass and EndMainPass
        render_app
            .add_systems(Render, prepare_fog_settings.in_set(RenderSet::Prepare))
            .add_render_graph_node::<ViewNodeRunner<FogNode>>(Core2d, FogNodeLabel)
            .add_render_graph_edges(Core2d, (Node2d::MainTransparentPass, FogNodeLabel, Node2d::EndMainPass));

        println!("已将迷雾节点添加到渲染图中，位于 MainTransparentPass 和 EndMainPass 之间");
        println!("Added fog node to render graph, between MainTransparentPass and EndMainPass");
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

        // 创建绑定组布局，需要纹理、采样器和迷雾设置
        // Create bind group layout with texture, sampler and fog settings
        let layout = render_device.create_bind_group_layout(
            "fog_bind_group_layout",
            &[
                // 输入纹理绑定
                // Input texture binding
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
                // 采样器绑定
                // Sampler binding
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                // 迷雾设置统一缓冲区
                // Fog settings uniform buffer
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: BufferSize::new(std::mem::size_of::<GpuFogSettings>() as u64),
                    },
                    count: None,
                },
            ],
        );

        // 创建线性过滤采样器
        // Create linear filtering sampler
        let sampler = render_device.create_sampler(&SamplerDescriptor {
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Nearest,
            ..default()
        });

        // 这部分代码已移至上方，使用更明确的着色器加载方式
        // This part of code has been moved above, using a more explicit shader loading method

        // 等待着色器资源加载完成
        // Wait for shader resource to be loaded
        let shader_handle = world
            .resource::<AssetServer>()
            .load::<Shader>("shaders/debug_red.wgsl");

        // 确保着色器已加载
        // Ensure shader is loaded
        println!("正在加载迷雾着色器...");
        println!("Loading fog shader...");

        let pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("fog_pipeline".into()),
            layout: vec![layout.clone()],
            vertex: fullscreen_shader_vertex_state(),
            fragment: Some(FragmentState {
                shader: shader_handle,
                shader_defs: vec![],
                entry_point: "fragment".into(),
                // 使用视图目标的标准格式
                // Use the standard format of the view target
                // 使用 Rgba8UnormSrgb 格式，根据先前的测试结果
                // Use Rgba8UnormSrgb format, based on previous test results
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::Rgba8UnormSrgb,
                    // 使用透明混合模式，确保迷雾效果正确混合
                    // Use alpha blending to ensure fog effect blends correctly
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: BlendFactor::SrcAlpha,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent {
                            src_factor: BlendFactor::SrcAlpha,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                    }),
                    // 使用正常的透明混合模式
                    // Use normal alpha blending
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
    center: Vec2,     // 迷雾中心位置 / fog center position
    density: f32,
    range: f32,       // 迷雾范围 / fog range
    time: f32,        // 时间（用于动画） / time (for animation)
    clear_radius: f32, // 相机周围的透明半径 / clear radius around camera
    clear_falloff: f32, // 边缘过渡效果 / edge falloff effect
    _padding3: f32,
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
    time: Res<Time>
) {
    // 只处理第一个相机
    // Only process the first camera
    let (view, camera, transform) = match views.iter().next() {
        Some(v) => v,
        None => return,
    };
    let camera_position = transform.translation().truncate();

    // 获取当前时间用于迷雾动画
    // Get current time for fog animation
    let elapsed_time = time.elapsed_secs();
    
    let settings = GpuFogSettings {
        color: fog_settings.color.to_linear().to_vec4().into(),
        center: camera_position,  // 使用相机位置作为迷雾中心 / Use camera position as fog center
        density: fog_settings.density,
        range: fog_settings.fog_range,
        time: elapsed_time,
        clear_radius: fog_settings.clear_radius,  // 相机周围的透明区域半径 / Clear radius around camera
        clear_falloff: fog_settings.clear_falloff, // 边缘过渡效果宽度 / Edge falloff width
        _padding3: 0.0,
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
                Some(entity) => {
                    println!("迷雾渲染系统找到了带有FogCameraMarker的相机实体");
                    entity
                }
                None => {
                    println!("迷雾渲染系统没有找到带有FogCameraMarker的相机实体");
                    return;
                }
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
        // 创建绑定组，包含纹理、采样器和迷雾设置
        // Create bind group with texture, sampler and fog settings
        println!("迷雾渲染系统创建绑定组");
        println!("Fog render system creating bind group");
        self.bind_group = Some(render_device.create_bind_group(
            "fog_bind_group",
            &fog_pipeline.layout,
            &[
                // 输入纹理 - 使用主纹理
                // Input texture - use main texture
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(view_target.main_texture_view()),
                },
                // 采样器
                // Sampler
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&fog_pipeline.sampler),
                },
                // 迷雾设置统一缓冲区
                // Fog settings uniform buffer
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Buffer(BufferBinding {
                        buffer: &fog_settings_uniform.buffer,
                        offset: 0,
                        size: None,
                    }),
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
        println!("迷雾渲染系统正在运行");
        let fog_pipeline = world.resource::<FogPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();

        let bind_group = match &self.bind_group {
            Some(bind_group) => bind_group,
            None => {
                println!("迷雾渲染系统没有绑定组");
                return Ok(());
            }
        };

        let pipeline = match pipeline_cache.get_render_pipeline(fog_pipeline.pipeline_id) {
            Some(pipeline) => pipeline,
            None => {
                println!("迷雾渲染系统没有找到渲染管线");
                return Ok(());
            }
        };
        println!("迷雾渲染系统找到了绑定组和渲染管线");
        // 使用 main_texture 作为渲染目标，确保效果会显示
        // Use main_texture as render target to ensure effect will be displayed
        println!("迷雾渲染系统开始渲染通道");
        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("fog_pass"),
            // 使用 post_process_write 作为渲染目标，这样我们可以将结果写入不同的纹理
            // Use post_process_write as render target so we can write to a different texture
            color_attachments: &[Some(RenderPassColorAttachment {
                view: view_target.post_process_write().destination,
                resolve_target: None,
                ops: Operations {
                    // 使用 Clear 操作确保渲染结果可见
                    // Use Clear operation to ensure rendering results are visible
                    load: LoadOp::Clear(Color::NONE.to_linear().into()),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        println!("迷雾渲染系统设置管线: {:?}", pipeline);
        render_pass.set_render_pipeline(pipeline);

        println!("迷雾渲染系统设置绑定组: {:?}", bind_group);
        render_pass.set_bind_group(0, bind_group, &[]);

        println!("迷雾渲染系统开始绘制三角形");
        render_pass.draw(0..3, 0..1);
        println!("迷雾渲染系统完成绘制");

        Ok(())
    }
}

impl FromWorld for FogNode {
    fn from_world(_world: &mut World) -> Self {
        Self::new()
    }
}
