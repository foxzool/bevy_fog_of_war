// fog_render/compute.rs
use bevy::prelude::*;
use bevy::render::render_graph::{Node, NodeRunError, RenderGraphContext, RenderLabel};
use bevy::render::render_resource::{BindGroupLayout, CachedComputePipelineId, ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, PipelineCache};
use bevy::render::renderer::{RenderContext, RenderDevice};

use super::prepare::{FogBindGroups, GpuChunkInfoBuffer}; // Import buffer to get chunk count / 导入缓冲区以获取区块数量
use super::{FOG_COMPUTE_SHADER_HANDLE, RenderFogMapSettings}; // Import shader handle / 导入 shader 句柄

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct FogComputeNodeLabel;

#[derive(Default)]
pub struct FogComputeNode;

#[derive(Resource)]
pub struct FogComputePipeline {
    pipeline_id: CachedComputePipelineId,
    compute_layout: BindGroupLayout, // Store layout used / 存储使用的布局
}

// System to initialize the compute pipeline / 初始化计算管线的系统
impl FromWorld for FogComputePipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let fog_bind_groups = world.resource::<FogBindGroups>();

        let compute_layout = fog_bind_groups
            .compute_layout
            .clone()
            .expect("Compute layout not created");

        let pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("fog_compute_pipeline".into()),
            layout: vec![compute_layout.clone()], // Use the prepared layout / 使用准备好的布局
            shader: FOG_COMPUTE_SHADER_HANDLE,
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
    // Input dependency (optional, ensures buffers are ready) / 输入依赖 (可选，确保缓冲区准备就绪)
    // fn input(&self) -> Vec<SlotInfo> { vec![] }
    // Output dependency (optional) / 输出依赖 (可选)
    // fn output(&self) -> Vec<SlotInfo> { vec![] }

    fn run(
        &self,
        _graph: &mut RenderGraphContext, // Use graph.view_entity() if needed / 如果需要使用 graph.view_entity()
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
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

        let texture_res = settings.0.texture_resolution_per_chunk;
        // Calculate workgroups needed / 计算所需的工作组
        // Example: One workgroup per chunk, 8x8 threads per workgroup
        // 示例: 每个区块一个工作组，每个工作组 8x8 线程
        // Adjust workgroup size in shader and here accordingly! / 相应地调整 shader 和此处的工作组大小！
        let workgroup_size_x = 8;
        let workgroup_size_y = 8;
        let workgroups_x = (texture_res.x + workgroup_size_x - 1) / workgroup_size_x;
        let workgroups_y = (texture_res.y + workgroup_size_y - 1) / workgroup_size_y;
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
