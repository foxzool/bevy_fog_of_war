use crate::chunk::ChunkManager;
use crate::fog_2d::{FogNode2dLabel, GpuChunks};
use crate::vision::{ChunkInfo, VisionTexture};
use bevy_app::{App, Plugin};
use bevy_asset::DirectAssetAccessExt;
use bevy_core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy_diagnostic::FrameCount;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryItem;
use bevy_ecs::system::lifetimeless::Read;
use bevy_render::{
    Extract, ExtractSchedule, Render, RenderApp, RenderSet,
    render_graph::{NodeRunError, RenderGraphApp, RenderGraphContext, ViewNode, ViewNodeRunner},
    render_resource::binding_types::{
        sampler, storage_buffer_read_only, texture_2d, uniform_buffer,
    },
    render_resource::{
        BindGroup, BindGroupEntries, ComputePassDescriptor, SamplerBindingType, TextureSampleType,
    },
    render_resource::{
        BindGroupLayout, BindGroupLayoutEntries, CachedComputePipelineId,
        ComputePipelineDescriptor, Extent3d, PipelineCache, Sampler, SamplerDescriptor,
        ShaderStages, StorageTextureAccess, TextureDescriptor, TextureDimension, TextureFormat,
        TextureUsages, binding_types::texture_storage_2d_array,
    },
    renderer::{RenderContext, RenderDevice},
    texture::{CachedTexture, TextureCache},
    view::{ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
};
use bevy_render_macros::RenderLabel;

const SHADER_ASSET_PATH: &str = "shaders/snapshot.wgsl";

pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<SnapshotTexture>()
            .add_systems(ExtractSchedule, prepare_snapshot_texture)
            .add_systems(
                Render,
                prepare_bind_group.in_set(RenderSet::PrepareBindGroups),
            );

        render_app
            .add_render_graph_node::<ViewNodeRunner<SnapshotNode>>(Core2d, SnapshotNodeLabel)
            // 修改 Render Graph 边：确保在主 Pass 之后运行，以便采样其结果
            // Modify Render Graph edge: Ensure it runs after the main pass to sample its result
            .add_render_graph_edges(
                Core2d,
                (
                    Node2d::MainTransparentPass,
                    FogNode2dLabel,
                    SnapshotNodeLabel,
                    Node2d::EndMainPass,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.init_resource::<SnapshotPipeline>();
    }
}

#[derive(Resource, Default)]
pub struct SnapshotTexture {
    pub write: Option<CachedTexture>,
    pub read: Option<CachedTexture>,
}

fn prepare_snapshot_texture(
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
    texture_descriptor.label = Some("snap_1_texture");
    let history_1_texture = texture_cache.get(&render_device, texture_descriptor.clone());

    texture_descriptor.label = Some("snap_2_texture");
    let history_2_texture = texture_cache.get(&render_device, texture_descriptor.clone());

    let texture = if frame_count.0 % 2 == 0 {
        SnapshotTexture {
            write: Some(history_1_texture),
            read: Some(history_2_texture),
        }
    } else {
        SnapshotTexture {
            write: Some(history_2_texture),
            read: Some(history_1_texture),
        }
    };

    commands.insert_resource(texture);
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct SnapshotNodeLabel;

#[derive(Default)]
struct SnapshotNode;

impl ViewNode for SnapshotNode {
    type ViewQuery = (Read<ViewUniformOffset>,);

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_uniform_offset,): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let pipeline_cache = world.resource::<PipelineCache>();
        let snapshot_pipeline = world.resource::<SnapshotPipeline>();
        let bind_group = world.resource::<SnapshotBindGroup>();

        let mut pass =
            render_context
                .command_encoder()
                .begin_compute_pass(&ComputePassDescriptor {
                    label: Some("snapshot_pass"),
                    timestamp_writes: None,
                });
        let Some(snapshot_pipeline) =
            pipeline_cache.get_compute_pipeline(snapshot_pipeline.pipeline_id)
        else {
            return Ok(());
        };

        pass.set_pipeline(snapshot_pipeline);
        pass.set_bind_group(0, &bind_group.0, &[view_uniform_offset.offset]);
        // compute_pass.set_bind_group(1, &bind_group.datadata, &[]);

        let workgroup_size = 16;
        // Compute dispatch size based on chunk width/height and number of layers
        let chunk_manager = world.resource::<ChunkManager>();
        let dispatch_x = (chunk_manager.chunk_size.x + workgroup_size - 1) / workgroup_size;
        let dispatch_y = (chunk_manager.chunk_size.y + workgroup_size - 1) / workgroup_size;
        let chunk_manager = world.resource::<ChunkManager>(); // 第二次获取，可以移除
        // The second acquisition can be removed

        pass.dispatch_workgroups(dispatch_x, dispatch_y, chunk_manager.chunk_in_views as u32);

        Ok(())
    }
}

#[derive(Resource)]
struct SnapshotPipeline {
    layout: BindGroupLayout,
    sampler: Sampler,
    pipeline_id: CachedComputePipelineId,
}

impl FromWorld for SnapshotPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let layout = render_device.create_bind_group_layout(
            "snapshot_bind_group_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    // @binding(0) ViewUniform
                    uniform_buffer::<ViewUniform>(true),
                    // @binding(1) ChunkInfo Buffer
                    storage_buffer_read_only::<ChunkInfo>(false), // 确保 ChunkInfo 定义与 Shader 一致
                    // @binding(2) Snapshot Write Texture
                    texture_storage_2d_array(
                        TextureFormat::Rgba8Unorm,
                        StorageTextureAccess::WriteOnly,
                    ),
                    // @binding(3) Source Texture (e.g., from ViewTarget)
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    // @binding(4) Source Sampler
                    sampler(SamplerBindingType::Filtering),
                    texture_storage_2d_array(
                        TextureFormat::R8Unorm,
                        StorageTextureAccess::ReadOnly,
                    ),
                ),
            ),
        );

        // 创建一个默认采样器
        // Create a default sampler
        let sampler = render_device.create_sampler(&SamplerDescriptor {
            label: Some("snapshot_source_sampler"),
            ..Default::default() // 使用默认设置，例如线性过滤
                                 // Use default settings, e.g., linear filtering
        });

        // Get the shader handle
        let shader = world.load_asset(SHADER_ASSET_PATH);
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("snapshot compute shader".into()),
            layout: vec![layout.clone()],
            push_constant_ranges: Vec::new(),
            shader: shader.clone(),
            shader_defs: Vec::new(),
            entry_point: "main".into(),
            zero_initialize_workgroup_memory: false,
        });

        Self {
            layout,
            sampler, // 存储采样器以备后用
            // Store the sampler for later use
            pipeline_id,
        }
    }
}

fn prepare_bind_group(
    pipeline: Res<SnapshotPipeline>,
    view_uniforms: Res<ViewUniforms>,
    render_device: Res<RenderDevice>,
    chunk_info: Res<GpuChunks>,
    snapshot_texture: Res<SnapshotTexture>,
    vision_texture: Res<VisionTexture>,
    view_targets: Query<&ViewTarget>,
    mut commands: Commands,
) {
    // 假设我们总是处理第一个（或唯一一个）ViewTarget
    // Assume we always process the first (or only) ViewTarget
    let Some(view_target) = view_targets.iter().next() else {
        // 没有找到 ViewTarget，无法进行快照
        // No ViewTarget found, cannot perform snapshot
        return;
    };

    let Some(chunk_info_buffer_binding) = chunk_info.buffer.as_ref().map(|b| b.as_entire_binding())
    else {
        return;
    };
    let Some(view_uniforms_binding) = view_uniforms.uniforms.binding() else {
        return;
    };

    let Some(snapshot_write) = &snapshot_texture.write else {
        return;
    };

    let Some(vision_read) = &vision_texture.read else {
        return;
    };

    // 获取源纹理视图
    // Get the source texture view
    let source_texture_view = view_target.main_texture_view();

    let bind_group = render_device.create_bind_group(
        "snapshot_bind_group", // 修改名称以更好地区分
        // Change name for better distinction
        &pipeline.layout,
        &BindGroupEntries::sequential((
            view_uniforms_binding,        // @binding(0)
            chunk_info_buffer_binding,    // @binding(1)
            &snapshot_write.default_view, // @binding(2)
            source_texture_view,          // @binding(3)
            &pipeline.sampler,            // @binding(4)
            &vision_read.default_view,    // @binding(5)
        )),
    );

    commands.insert_resource(SnapshotBindGroup(bind_group));
}

#[derive(Resource)]
struct SnapshotBindGroup(BindGroup);
