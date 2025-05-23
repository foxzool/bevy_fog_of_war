// src/render/compute.rs

use super::prepare::{FogBindGroups, GpuChunkInfoBuffer};
use crate::render::extract::{ChunkComputeData, RenderFogMapSettings, VisionSourceData};
use bevy::render::render_resource::StorageTextureAccess::WriteOnly;
use bevy::{
    prelude::*,
    render::{
        render_graph::{Node, NodeRunError, RenderGraphContext, RenderLabel},
        render_resource::StorageTextureAccess::ReadWrite,
        render_resource::binding_types::{
            storage_buffer_read_only, texture_storage_2d_array, uniform_buffer,
        },
        render_resource::{
            BindGroupLayout, BindGroupLayoutEntries, CachedComputePipelineId,
            ComputePassDescriptor, ComputePipelineDescriptor, PipelineCache, ShaderStages,
            TextureFormat,
        },
        renderer::{RenderContext, RenderDevice},
    },
};
use crate::snapshot::SnapshotCamera;

const SHADER_ASSET_PATH: &str = "shaders/fog_compute.wgsl";

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct FogComputeNodeLabel;

#[derive(Default)]
pub struct FogComputeNode;

#[derive(Resource)]
pub struct FogComputePipeline {
    pub pipeline_id: CachedComputePipelineId,
    pub compute_layout: BindGroupLayout, // Store layout used / 存储使用的布局
}

// System to initialize the compute pipeline / 初始化计算管线的系统
impl FromWorld for FogComputePipeline {
    fn from_world(world: &mut World) -> Self {
        let pipeline_cache = world.resource::<PipelineCache>();

        let render_device = world.resource::<RenderDevice>();

        let compute_layout = render_device.create_bind_group_layout(
            "fog_compute_bind_group_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    texture_storage_2d_array(TextureFormat::R8Unorm, ReadWrite), // 0
                    texture_storage_2d_array(TextureFormat::R8Unorm, WriteOnly), // 1
                    storage_buffer_read_only::<VisionSourceData>(false),         // 2
                    storage_buffer_read_only::<ChunkComputeData>(false),         // 3
                    uniform_buffer::<RenderFogMapSettings>(false),               // 4
                ),
            ),
        );

        let shader = world.load_asset(SHADER_ASSET_PATH);

        let pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("fog_compute_pipeline".into()),
            layout: vec![compute_layout.clone()], // Use the prepared layout / 使用准备好的布局
            shader,
            shader_defs: vec![], // Add shader defs if needed / 如果需要添加 shader defs
            entry_point: "main".into(),
            push_constant_ranges: vec![],
            zero_initialize_workgroup_memory: false,
        });

        FogComputePipeline {
            pipeline_id,
            compute_layout,
        }
    }
}

impl Node for FogComputeNode {
    fn run(
        &self,
        graph: &mut RenderGraphContext, 
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let view_entity = graph.view_entity();
        
        if world.get::<SnapshotCamera>(view_entity).is_some() {
            return Ok(());
        }
        
        let fog_bind_groups = world.resource::<FogBindGroups>();
        let compute_pipeline = world.resource::<FogComputePipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let chunk_buffer = world.resource::<GpuChunkInfoBuffer>();
        let settings = world.resource::<RenderFogMapSettings>();

        let Some(pipeline) = pipeline_cache.get_compute_pipeline(compute_pipeline.pipeline_id)
        else {
            // Pipeline not compiled yet / 管线尚未编译
            return Ok(());
        };

        let Some(compute_bind_group) = &fog_bind_groups.compute else {
            // Bind group not ready / 绑定组未准备好
            // info!("Compute bind group not ready.");
            return Ok(());
        };

        let chunk_count = chunk_buffer.capacity; // Number of active GPU chunks / 活动 GPU 区块的数量
        if chunk_count == 0 {
            return Ok(()); // No work to do / 无需工作
        }

        let texture_res = settings.texture_resolution_per_chunk;
        let workgroup_size_x = 8;
        let workgroup_size_y = 8;
        let workgroups_x = texture_res.x.div_ceil(workgroup_size_x);
        let workgroups_y = texture_res.y.div_ceil(workgroup_size_y);
        // Dispatch per chunk / 按区块分派
        let workgroups_z = chunk_count as u32;

        let mut compute_pass =
            render_context
                .command_encoder()
                .begin_compute_pass(&ComputePassDescriptor {
                    label: Some("fog_compute_pass"),
                    timestamp_writes: None,
                });

        compute_pass.set_pipeline(pipeline);
        compute_pass.set_bind_group(0, compute_bind_group, &[]);
        compute_pass.dispatch_workgroups(workgroups_x, workgroups_y, workgroups_z);

        // info!("Dispatched compute shader: {}x{}x{}", workgroups_x, workgroups_y, workgroups_z);

        Ok(())
    }
}
