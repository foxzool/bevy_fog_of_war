use crate::chunk::{ChunkManager, InCameraView, MapChunk};
use crate::fog_2d::GpuChunks;
use crate::prelude::FogOfWarCamera;
use bevy_app::{App, Plugin};
use bevy_asset::AssetServer;
use bevy_core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy_diagnostic::FrameCount;
use bevy_ecs::{prelude::*, query::QueryItem, system::lifetimeless::Read};
use bevy_encase_derive::ShaderType;
use bevy_log::warn;
use bevy_math::{IVec2, UVec2, Vec2};
use bevy_reflect::Reflect;
use bevy_render::extract_component::ExtractComponentPlugin;
use bevy_render::prelude::ViewVisibility;
use bevy_render::render_resource::binding_types::{sampler, texture_2d};
use bevy_render::render_resource::{
    Buffer, Sampler, SamplerBindingType, SamplerDescriptor, TextureSampleType,
};
use bevy_render::view::ViewTarget;
use bevy_render::{
    Extract, ExtractSchedule, Render, RenderApp, RenderSet,
    render_graph::{NodeRunError, RenderGraphApp, RenderGraphContext, ViewNode, ViewNodeRunner},
    render_resource::{
        BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, BufferInitDescriptor,
        BufferUsages, CachedComputePipelineId, ComputePassDescriptor, ComputePipelineDescriptor,
        Extent3d, PipelineCache, ShaderStages, StorageTextureAccess, TextureDescriptor,
        TextureDimension, TextureFormat, TextureUsages, UniformBuffer,
        binding_types::{storage_buffer_read_only, texture_storage_2d_array, uniform_buffer},
    },
    renderer::{RenderContext, RenderDevice, RenderQueue},
    texture::{CachedTexture, TextureCache},
    view::{ViewUniform, ViewUniformOffset, ViewUniforms},
};
use bevy_render_macros::{ExtractComponent, RenderLabel};
use bevy_transform::components::GlobalTransform;
use bytemuck::{Pod, Zeroable};

/// Render graph node label for the vision compute pass.
/// 视野计算通道的渲染图节点标签。
#[derive(RenderLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct VisionComputeNodeLabel;

// --- Plugin ---

/// Plugin responsible for setting up the vision compute shader resources and pipeline.
/// 负责设置视野计算着色器资源和管线的插件。
pub struct VisionComputePlugin;

impl Plugin for VisionComputePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<VisionProvider>::default());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<VisionParamsResource>()
            .init_resource::<GpuChunks>()
            .init_resource::<ChunkMetaBuffer>()
            // .init_resource::<VisionTexture>()
            .init_resource::<ExploredTexture>()
            .add_systems(
                ExtractSchedule,
                (
                    prepare_explored_texture,
                    prepare_vision_texture.run_if(not(resource_exists::<VisionTexture>)),
                    update_vision_params,
                    prepare_chunk_info,
                )
                    .chain(),
            )
            .add_systems(
                Render,
                prepare_bind_group.in_set(RenderSet::PrepareBindGroups),
            );

        render_app
            .add_render_graph_node::<ViewNodeRunner<VisionComputeNode>>(
                Core2d,
                VisionComputeNodeLabel,
            )
            .add_render_graph_edges(
                Core2d,
                (
                    Node2d::MainTransparentPass,
                    VisionComputeNodeLabel,
                    Node2d::EndMainPass,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        // Initialize the pipeline resource here where AssetServer is reliably available
        // 在这里初始化管线资源，确保 AssetServer 可靠可用
        render_app.init_resource::<VisionComputePipeline>();
    }
}

#[derive(Resource, Default)]
pub struct VisionTexture {
    pub write: Option<CachedTexture>,
}

fn prepare_vision_texture(
    mut commands: Commands,
    mut texture_cache: ResMut<TextureCache>,
    render_device: Res<RenderDevice>,
    chunk_manager: Extract<Res<ChunkManager>>,
) {
    let size = Extent3d {
        width: chunk_manager.chunk_size.x,
        height: chunk_manager.chunk_size.y,
        depth_or_array_layers: chunk_manager.max_layer_count,
    };
    let mut texture_descriptor = TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::R8Unorm,
        usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };
    texture_descriptor.label = Some("vision_history_1_texture");
    let history_1_texture = texture_cache.get(&render_device, texture_descriptor.clone());

    let texture = VisionTexture {
        write: Some(history_1_texture),
    };

    commands.insert_resource(texture);
}

#[derive(Resource, Default)]
pub struct ExploredTexture {
    pub write: Option<CachedTexture>,
    pub read: Option<CachedTexture>,
}

fn prepare_explored_texture(
    mut texture_cache: ResMut<TextureCache>,
    render_device: Res<RenderDevice>,
    frame_count: Extract<Res<FrameCount>>,
    chunk_manager: Extract<Res<ChunkManager>>,
    mut commands: Commands,
) {
    let width = chunk_manager.chunk_size.x * chunk_manager.tile_size as u32;
    let height = chunk_manager.chunk_size.y * chunk_manager.tile_size as u32;
    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: chunk_manager.max_layer_count,
    };

    let mut texture_descriptor = TextureDescriptor {
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::COPY_DST | TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
        label: None,
        view_formats: &[],
    };
    texture_descriptor.label = Some("explored_history_1_texture");
    let history_1_texture = texture_cache.get(&render_device, texture_descriptor.clone());

    texture_descriptor.label = Some("explored_history_2_texture");
    let history_2_texture = texture_cache.get(&render_device, texture_descriptor.clone());

    let texture = if frame_count.0 % 2 == 0 {
        ExploredTexture {
            write: Some(history_1_texture),
            read: Some(history_2_texture),
        }
    } else {
        ExploredTexture {
            write: Some(history_2_texture),
            read: Some(history_1_texture),
        }
    };

    commands.insert_resource(texture);
}

// --- Pipeline Resource ---

/// Resource holding the compute pipeline and its layout.
/// 保存计算管线及其布局的资源。
#[derive(Resource)]
pub struct VisionComputePipeline {
    pub pipeline: CachedComputePipelineId,
    pub sampler: Sampler,
    // pub view_group_layout: BindGroupLayout,
    pub bind_group_layout: BindGroupLayout,
}

impl FromWorld for VisionComputePipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let shader = world.resource::<AssetServer>().load("shaders/vision.wgsl");

        // Layout: view uniform, vision params, chunk info, texture write
        let bind_group_layout = render_device.create_bind_group_layout(
            Some("vision_compute_bind_group_layout"),
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    uniform_buffer::<ViewUniform>(true),                // 0
                    storage_buffer_read_only::<GpuVisionParams>(false), // 1
                    storage_buffer_read_only::<ChunkInfo>(false),       // 2
                    texture_storage_2d_array(
                        TextureFormat::R8Unorm,
                        StorageTextureAccess::WriteOnly,
                    ), // 3 vision_texture_write
                    uniform_buffer::<ChunkMeta>(false),                 // 4
                    texture_storage_2d_array(
                        TextureFormat::Rgba8Unorm,
                        StorageTextureAccess::ReadOnly,
                    ), // 5 history_read
                    texture_storage_2d_array(
                        TextureFormat::Rgba8Unorm,
                        StorageTextureAccess::WriteOnly,
                    ), // 6 history_write
                    // @binding(3) Source Texture (e.g., from ViewTarget)
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    // @binding(4) Source Sampler
                    sampler(SamplerBindingType::Filtering),
                ),
            ),
        );

        // Create the compute pipeline
        // 创建计算管线
        let pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("vision_compute_pipeline".into()),
            layout: vec![bind_group_layout.clone()],
            push_constant_ranges: Vec::new(),
            shader,
            shader_defs: vec![],
            entry_point: "main".into(),
            zero_initialize_workgroup_memory: false,
        });
        let sampler = render_device.create_sampler(&SamplerDescriptor {
            label: Some("vision_source_sampler"),
            ..Default::default()
        });

        VisionComputePipeline {
            pipeline,
            sampler,
            bind_group_layout,
        }
    }
}

#[derive(Clone, Copy, ShaderType, Pod, Zeroable)]
#[repr(C)]
pub struct ChunkMeta {
    pub chunks_per_row: u32,
    pub chunk_size: u32,
}

#[derive(Default, Resource)]
pub struct ChunkMetaBuffer {
    pub buffer: Option<UniformBuffer<ChunkMeta>>,
}

// --- Bind Group Resource ---

/// Resource to hold the bind group for the compute shader.
/// 用于保存计算着色器绑定组的资源。
#[derive(Resource)]
struct VisionComputeBindGroup(BindGroup);

// --- Systems ---

/// System to prepare the bind group for the compute shader.
/// Runs in the Render app.
/// 准备计算着色器绑定组的系统。
/// 在 Render 应用中运行。
fn prepare_bind_group(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline: Res<VisionComputePipeline>,
    vision_params: Res<VisionParamsResource>,
    chunk_info: Res<GpuChunks>,
    view_uniforms: Res<ViewUniforms>,
    vision_texture: Res<VisionTexture>,
    chunk_meta_buffer: Res<ChunkMetaBuffer>,
    view_targets: Query<&ViewTarget, With<FogOfWarCamera>>,
    explored_texture: Res<ExploredTexture>,
) {
    let Some(view_target) = view_targets.iter().next() else {
        return;
    };

    let Some(vision_buffer_binding) = vision_params.buffer.as_ref().map(|b| b.as_entire_binding())
    else {
        warn!("VisionParamsResource buffer is missing, skipping compute bind group creation.");
        return;
    };
    let Some(chunk_info_buffer_binding) = chunk_info.buffer.as_ref().map(|b| b.as_entire_binding())
    else {
        warn!("ChunkInfoResource buffer is missing, skipping compute bind group creation.");
        return;
    };

    let Some(view_uniforms_binding) = view_uniforms.uniforms.binding() else {
        return;
    };

    let Some(chunk_meta_binding) = chunk_meta_buffer
        .buffer
        .as_ref()
        .map(|b| b.binding())
        .flatten()
    else {
        return;
    };

    let (Some(explored_read), Some(explored_write)) =
        (&explored_texture.read, &explored_texture.write)
    else {
        return;
    };

    let Some(vision_write) = &vision_texture.write else {
        return;
    };

    let source_texture_view = view_target.main_texture_view();

    let bind_group = render_device.create_bind_group(
        "vision_compute_bind_group",
        &pipeline.bind_group_layout,
        &BindGroupEntries::sequential((
            view_uniforms_binding,        // 0
            vision_buffer_binding,        // 1
            chunk_info_buffer_binding,    // 2
            &vision_write.default_view,   // 3
            chunk_meta_binding,           // 4
            &explored_read.default_view,  // 5
            &explored_write.default_view, // 6
            source_texture_view,          // 7
            &pipeline.sampler,            // 8
        )),
    );

    commands.insert_resource(VisionComputeBindGroup(bind_group));
}

// --- Render Graph Node ---

/// Node for dispatching the vision compute shader.
/// 用于调度视野计算着色器的节点。
#[derive(Default)]
pub struct VisionComputeNode;

impl ViewNode for VisionComputeNode {
    type ViewQuery = (Read<ViewUniformOffset>,);

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_uniform_offset,): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let Some(pipeline) = world.get_resource::<VisionComputePipeline>() else {
            // info!("Skipping vision compute pass: Pipeline resource not available.");
            return Ok(());
        };
        let Some(bind_group) = world.get_resource::<VisionComputeBindGroup>() else {
            // warn!("Skipping vision compute pass: Bind group not available.");
            return Ok(());
        };
        let pipeline_cache = world.resource::<PipelineCache>();
        let Some(compute_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.pipeline) else {
            warn!("Skipping vision compute pass: Pipeline not compiled yet.");
            return Ok(());
        };

        let mut compute_pass =
            render_context
                .command_encoder()
                .begin_compute_pass(&ComputePassDescriptor {
                    label: Some("vision_compute_pass"),
                    timestamp_writes: None,
                });

        compute_pass.set_pipeline(compute_pipeline);
        compute_pass.set_bind_group(0, &bind_group.0, &[view_uniform_offset.offset]);

        let workgroup_size = 16;
        // Compute dispatch size based on chunk width/height and number of layers
        let chunk_manager = world.resource::<ChunkManager>();
        let dispatch_x = (chunk_manager.chunk_size.x + workgroup_size - 1) / workgroup_size;
        let dispatch_y = (chunk_manager.chunk_size.y + workgroup_size - 1) / workgroup_size;
        let chunk_manager = world.resource::<ChunkManager>();

        compute_pass.dispatch_workgroups(
            dispatch_x,
            dispatch_y,
            chunk_manager.chunk_in_views as u32,
        );

        Ok(())
    }
}

// ...
/// GPU上的Chunk信息表示
/// GPU representation of chunk information
#[derive(ShaderType, Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct ChunkInfo {
    pub coord: IVec2,    // 区块坐标 / chunk coordinates
    pub world_min: Vec2, // 世界空间边界最小点 / world space minimum point
    pub world_max: Vec2, // 世界空间边界最大点 / world space maximum point
    pub size: UVec2,     // 区块尺寸 / chunk size
    pub layer_index: u32,
    // Add padding to match WGSL std430 alignment requirements (struct size should be multiple of
    // 8)
    pub _padding: u32, // Add 4 bytes padding
}

/// 准备Chunk信息缓冲区系统
/// Prepare chunk information buffer system
pub fn prepare_chunk_info(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    chunk_manager: Extract<Res<ChunkManager>>,
    chunks_query: Extract<Query<&MapChunk, With<InCameraView>>>,
) {
    let mut chunks_in_view: Vec<&MapChunk> = chunks_query
        .iter()
        .filter(|chunk| chunk.layer_index.is_some())
        .map(|chunk| chunk)
        .collect();

    chunks_in_view.sort_by_key(|chunk| chunk.layer_index);

    let chunk_count = chunks_in_view.len();

    // 创建GPU数据
    // Create GPU data
    let mut gpu_chunks = Vec::with_capacity(chunk_count);

    for chunk in chunks_in_view {
        if let Some(index) = chunk.layer_index {
            let gpu_chunk = ChunkInfo {
                coord: chunk.chunk_coord,
                world_min: chunk.world_bounds.min,
                world_max: chunk.world_bounds.max,
                size: chunk.size,
                layer_index: index,
                _padding: 0, // Initialize padding
            };

            gpu_chunks.push(gpu_chunk);
        }
    }

    if gpu_chunks.is_empty() {
        gpu_chunks.push(ChunkInfo {
            coord: IVec2::ZERO,
            world_min: Vec2::ZERO,
            world_max: Vec2::ZERO,
            size: UVec2::ZERO,
            layer_index: 0,
            _padding: 0,
        });
    }

    let mut index_set = std::collections::HashSet::new();
    for chunk in &gpu_chunks {
        if !index_set.insert(chunk.layer_index) {
            warn!("Duplicate layer_index found: {}", chunk.layer_index);
        }
    }

    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("chunk_info_buffer"),
        contents: bytemuck::cast_slice(&gpu_chunks),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
    });

    let chunk_info_resource = GpuChunks {
        buffer: Some(buffer),
    };

    // 插入资源
    // Insert resource
    commands.insert_resource(chunk_info_resource);

    let chunk_meta = ChunkMeta {
        chunks_per_row: chunk_manager.chunks_per_row as u32,
        chunk_size: chunk_manager.chunk_size.x,
    };

    let mut buffer = UniformBuffer::from(chunk_meta);

    buffer.write_buffer(&render_device, &render_queue);
    commands.insert_resource(ChunkMetaBuffer {
        buffer: Some(buffer),
    });
}

/// 视野提供者组件
/// Vision provider component
#[derive(Component, Reflect, ExtractComponent, Clone)]
#[require(InCameraView)]
pub struct VisionProvider {
    /// 视野范围（世界单位）
    /// Vision range (world units)
    pub range: f32,
}

// 视野源参数在 GPU 中的表示
// GPU representation of vision source parameters
#[derive(ShaderType, Pod, Zeroable, Clone, Copy, Debug)]
#[repr(C)] // Ensure C-compatible layout
pub struct GpuVisionSource {
    pub position: Vec2, // 8 bytes
    pub radius: f32,    // 4 bytes
    pub _padding: f32,  // 4 bytes padding, total 16 bytes to match WGSL
}

// 视野参数在 GPU 中的表示
// GPU representation of vision parameters
#[derive(Debug, Clone, Copy, ShaderType, Pod, Zeroable)]
#[repr(C)] // Ensure C-compatible layout
pub struct GpuVisionParams {
    // Number of vision sources (u32)
    pub count: u32,         // 4 bytes
    pub _padding: [u32; 3], // 12 bytes padding to align sources array to 16 bytes
    // Use a large fixed-size array or a dynamically sized buffer approach
    // Needs alignment considerations for WGSL
    // Example: Use Vec in staging buffer, copy to fixed-size array in GPU buffer if possible,
    pub sources: [GpuVisionSource; 16], // Example: fixed-size array of 16 sources
}

// 视野参数资源
// Vision parameters resource
#[derive(Resource, Default)]
pub struct VisionParamsResource {
    pub buffer: Option<Buffer>,
}

// 更新视野参数的 system
// System for updating vision parameters
pub fn update_vision_params(
    mut vision_params: ResMut<VisionParamsResource>,
    render_device: Res<RenderDevice>,
    query: Extract<Query<(&GlobalTransform, &VisionProvider, &ViewVisibility)>>,
) {
    let mut sources = [GpuVisionSource {
        position: Vec2::ZERO,
        radius: 0.0,
        _padding: 0.0,
    }; 16];
    let mut count = 0;
    for (transform, provider, vis) in query.iter().take(16) {
        if vis.get() {
            sources[count] = GpuVisionSource {
                position: transform.translation().truncate(),
                radius: provider.range,
                _padding: 0.0,
            };
            count += 1;
        }
    }

    let params = GpuVisionParams {
        count: count as u32,
        _padding: [0; 3],
        sources,
    };

    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("vision_params_buffer"),
        contents: bytemuck::cast_slice(&[params]),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
    });
    vision_params.buffer = Some(buffer);
}
