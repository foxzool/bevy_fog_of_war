use bevy::asset::weak_handle;
// fog_render/mod.rs
use bevy::core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy::prelude::*;
use bevy::render::render_graph::{RenderGraphApp, ViewNodeRunner};
use bevy::render::render_resource::SpecializedRenderPipelines;
use bevy::render::{Render, RenderApp, RenderSet};

// Import submodules for organization / 导入子模块以组织代码
mod compute;
mod extract;
mod overlay;
mod prepare;
mod snapshot; // Contains snapshot node and related logic / 包含快照节点和相关逻辑

// Re-export relevant items / 重新导出相关项
pub use compute::FogComputeNode;
pub use extract::RenderFogMapSettings;
// Make extracted settings accessible / 使提取的设置可访问
pub use overlay::FogOverlayNode;
pub use prepare::{
    FogBindGroups, FogUniforms, GpuChunkInfoBuffer, OverlayChunkMappingBuffer, VisionSourceBuffer,
};

pub const FOG_COMPUTE_SHADER_HANDLE: Handle<Shader> =
    weak_handle!("c79464f5-7e93-419e-88ec-871c9ad12247");
pub const FOG_OVERLAY_SHADER_HANDLE: Handle<Shader> =
    weak_handle!("f40f9e67-6ba7-4277-93cd-718c6ded2786");

pub struct FogOfWarRenderPlugin;

impl Plugin for FogOfWarRenderPlugin {
    fn build(&self, app: &mut App) {
        // Load shaders / 加载着色器
        let mut shaders = app.world_mut().resource_mut::<Assets<Shader>>();
        shaders.insert(
            &FOG_COMPUTE_SHADER_HANDLE,
            Shader::from_wgsl(include_str!("shaders/fog_compute.wgsl"), "fog_compute.wgsl"),
        );
        shaders.insert(
            &FOG_OVERLAY_SHADER_HANDLE,
            Shader::from_wgsl(include_str!("shaders/fog_overlay.wgsl"), "fog_overlay.wgsl"),
        );

        // Get Render App / 获取 Render App
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        // Add systems and resources to Render App / 向 Render App 添加系统和资源
        render_app
            // Resources for extracted data / 用于提取数据的资源
            .init_resource::<extract::ExtractedVisionSources>()
            .init_resource::<extract::ExtractedGpuChunkData>()
            .init_resource::<extract::SnapshotRequestQueue>()
            // Resources for prepared GPU data / 用于准备好的 GPU 数据的资源
            .init_resource::<FogUniforms>()
            .init_resource::<VisionSourceBuffer>()
            .init_resource::<GpuChunkInfoBuffer>()
            .init_resource::<OverlayChunkMappingBuffer>()
            .init_resource::<FogBindGroups>()
            .init_resource::<SpecializedRenderPipelines<overlay::FogOverlayPipeline>>() // For overlay pipeline cache / 用于覆盖管线缓存
            // .init_resource::<SpecializedRenderPipelines<snapshot::SnapshotPipeline>>() // For snapshot pipeline cache / 用于快照管线缓存
            // Extraction systems (Main World -> Render World) / 提取系统 (主世界 -> 渲染世界)
            .add_systems(
                ExtractSchedule,
                (
                    extract::extract_fog_settings,
                    extract::extract_vision_sources,
                    extract::extract_gpu_chunk_data,
                    extract::extract_snapshot_requests,
                    extract::extract_texture_handles, // Ensure handles are available / 确保句柄可用
                ),
            )
            // Prepare systems (Create/Update GPU buffers and bind groups) / 准备系统 (创建/更新 GPU 缓冲区和绑定组)
            .add_systems(
                Render,
                (
                    prepare::prepare_fog_uniforms,
                    prepare::prepare_vision_source_buffer,
                    prepare::prepare_gpu_chunk_buffer,
                    prepare::prepare_overlay_chunk_mapping_buffer,
                    prepare::prepare_fog_bind_groups,
                )
                    .in_set(RenderSet::PrepareBindGroups), // Run in the correct stage / 在正确的阶段运行
            )
            // Queue systems (Prepare pipelines) / 排队系统 (准备管线)
            .add_systems(
                Render,
                (
                    overlay::queue_fog_overlay_pipelines,
                    // snapshot::queue_snapshot_pipelines, // Queue snapshot pipelines / 排队快照管线
                )
                    .in_set(RenderSet::Queue),
            );

        // Add Render Graph nodes / 添加 Render Graph 节点
        render_app
            .add_render_graph_node::<FogComputeNode>(Core2d, compute::FogComputeNodeLabel)
            // .add_render_graph_node::<ViewNodeRunner<SnapshotNode>>(
            //     Core2d,
            //     snapshot::SnapshotNodeLabel,
            // ) // Use ViewNode for camera access / 使用 ViewNode 访问相机
            .add_render_graph_node::<ViewNodeRunner<FogOverlayNode>>(
                Core2d,
                overlay::FogOverlayNodeLabel,
            ); // Use ViewNode for camera access / 使用 ViewNode 访问相机

        // Add Render Graph edges (define dependencies) / 添加 Render Graph 边 (定义依赖)
        render_app.add_render_graph_edges(
            Core2d,
            (
                // Run compute shader after buffer preparation / 在缓冲区准备后运行 compute shader
                Node2d::StartMainPass, // Or a PrepareNode if more fine-grained control needed / 或 PrepareNode 如果需要更细粒度控制
                compute::FogComputeNodeLabel,
                // Run snapshotting after compute (fog state might influence snapshot?) or in parallel
                // 在计算后运行快照 (雾状态可能影响快照?) 或并行运行
                // If snapshot needs main pass depth, it runs after StartMainPass too
                // 如果快照需要主通道深度，它也在 StartMainPass 之后运行
                Node2d::StartMainPass,
                // snapshot::SnapshotNodeLabel,
                // Run overlay after compute, snapshot, and the main 2D pass
                // 在计算、快照和主 2D 通道之后运行覆盖
                compute::FogComputeNodeLabel,
                overlay::FogOverlayNodeLabel,
                // snapshot::SnapshotNodeLabel,
                overlay::FogOverlayNodeLabel,
                Node2d::EndMainPass, // Ensure main pass finishes before overlay / 确保主通道在覆盖之前完成
                overlay::FogOverlayNodeLabel,
                // Connect overlay node to the end of the 2D graph (before UI)
                // 将覆盖节点连接到 2D 图的末尾 (在 UI 之前)
                overlay::FogOverlayNodeLabel,
                Node2d::EndMainPass, // Or directly to Tonemapping if needed / 或直接到 Tonemapping 如果需要
            ),
        );
    }

    // Finish building Render App (e.g., initializing pipelines)
    // 完成 Render App 构建 (例如，初始化管线)
    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<compute::FogComputePipeline>() // Initialize compute pipeline / 初始化计算管线
            .init_resource::<overlay::FogOverlayPipeline>(); // Initialize overlay pipeline / 初始化覆盖管线
        // .init_resource::<snapshot::SnapshotPipeline>(); // Initialize snapshot pipeline / 初始化快照管线
    }
}
