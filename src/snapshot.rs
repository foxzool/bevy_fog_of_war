use crate::chunk::ChunkManager;
use crate::fog::GpuChunks;
use crate::render::FogNode2dLabel;
use crate::vision_compute::ChunkInfo;
use bevy_app::{App, Plugin};
use bevy_asset::DirectAssetAccessExt;
use bevy_core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy_diagnostic::FrameCount;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryItem;
use bevy_ecs::system::lifetimeless::Read;
use bevy_render::render_resource::binding_types::{storage_buffer_read_only, uniform_buffer};
use bevy_render::render_resource::{BindGroup, BindGroupEntries, ComputePassDescriptor};
use bevy_render::view::{ViewUniform, ViewUniformOffset, ViewUniforms};
use bevy_render::{
    render_graph::{NodeRunError, RenderGraphApp, RenderGraphContext, ViewNode, ViewNodeRunner}, render_resource::{
        binding_types::texture_storage_2d_array, BindGroupLayout, BindGroupLayoutEntries,
        CachedComputePipelineId, ComputePipelineDescriptor, Extent3d, PipelineCache, Sampler,
        SamplerDescriptor, ShaderStages, StorageTextureAccess, TextureDescriptor, TextureDimension,
        TextureFormat, TextureUsages,
    }, renderer::{RenderContext, RenderDevice}, texture::{CachedTexture, TextureCache}, Extract,
    ExtractSchedule,
    Render,
    RenderApp,
    RenderSet,
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
            .add_render_graph_edges(
                Core2d,
                (FogNode2dLabel, SnapshotNodeLabel, Node2d::EndMainPass),
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
struct SnapshotTexture {
    write: Option<CachedTexture>,
    read: Option<CachedTexture>,
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
        let chunk_manager = world.resource::<ChunkManager>();

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
                // The layout entries will only be visible in the fragment stage
                ShaderStages::COMPUTE,
                (
                    uniform_buffer::<ViewUniform>(true),
                    storage_buffer_read_only::<ChunkInfo>(false),
                    texture_storage_2d_array(
                        TextureFormat::Rgba8Unorm,
                        StorageTextureAccess::WriteOnly,
                    ), // 3
                ),
            ),
        );

        let sampler = render_device.create_sampler(&SamplerDescriptor::default());

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
            sampler,
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
    mut commands: Commands,
) {
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

    let bind_group = render_device.create_bind_group(
        "vision_compute_bind_group",
        &pipeline.layout,
        &BindGroupEntries::sequential((
            view_uniforms_binding,
            chunk_info_buffer_binding,
            &snapshot_write.default_view,
        )),
    );

    commands.insert_resource(SnapshotBindGroup(bind_group));
}

#[derive(Resource)]
struct SnapshotBindGroup(BindGroup);
