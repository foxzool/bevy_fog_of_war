use crate::prelude::*;
use bevy::{
    core_pipeline::core_2d::graph::{Core2d, Node2d},
    render::{
        Render, RenderApp, RenderSet,
        render_graph::{RenderGraphApp, ViewNodeRunner},
        renderer::render_system,
    },
};

mod compute;
mod extract;
mod overlay;
mod prepare;
mod transfer;

use crate::render::transfer::{CpuToGpuRequests, GpuToCpuActiveCopies};
pub use compute::{FogComputeNode, FogComputeNodeLabel};
pub use extract::{RenderFogMapSettings, RenderSnapshotTempTexture, RenderSnapshotTexture};
pub use overlay::{FogOverlayNode, FogOverlayNodeLabel};
pub use prepare::{
    FogBindGroups, FogUniforms, GpuChunkInfoBuffer, OverlayChunkMappingBuffer, VisionSourceBuffer,
};

pub struct FogOfWarRenderPlugin;

impl Plugin for FogOfWarRenderPlugin {
    fn build(&self, app: &mut App) {
        // Get Render App / 获取 Render App
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        // Add systems and resources to Render App / 向 Render App 添加系统和资源
        render_app
            // Resources for extracted data / 用于提取数据的资源
            .init_resource::<extract::ExtractedVisionSources>()
            .init_resource::<extract::ExtractedGpuChunkData>()
            .init_resource::<FogUniforms>()
            .init_resource::<VisionSourceBuffer>()
            .init_resource::<GpuToCpuActiveCopies>()
            .init_resource::<GpuChunkInfoBuffer>()
            .init_resource::<OverlayChunkMappingBuffer>()
            .init_resource::<FogBindGroups>()
            .init_resource::<CpuToGpuRequests>();

        // Extraction systems (Main World -> Render World) / 提取系统 (主世界 -> 渲染世界)
        render_app
            .add_systems(
                ExtractSchedule,
                (
                    extract::extract_fog_settings,
                    extract::extract_vision_sources,
                    extract::extract_gpu_chunk_data,
                    extract::extract_texture_handles,
                    extract::extract_snapshot_visible_entities,
                    transfer::check_and_process_mapped_buffers,
                    transfer::check_cpu_to_gpu_request,
                ),
            )
            .add_systems(
                Render,
                (
                    // CPU -> GPU
                    (transfer::process_cpu_to_gpu_copies,).in_set(RenderSet::PrepareResources),
                    // GPU -> CPU - Stage 1: Initiate copy and request map
                    // Run this after rendering/compute that populates the textures for the current frame.
                    // CleanupCommands is a good place.
                    (
                        transfer::initiate_gpu_to_cpu_copies_and_request_map,
                        transfer::map_buffers,
                    )
                        .after(render_system)
                        .in_set(RenderSet::Render),
                    // GPU -> CPU - Stage 2: Check for mapped buffers and process them
                    // Run this in the *next* frame, typically early (e.g., Prepare).
                    // Or, if your game loop/framerate allows, and map_async is fast on your GPU,
                    // you *could* try to check it at the very end of the current frame or start of next.
                    // For clarity and robustness with async, processing in the next frame's Prepare is safer.
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
                    .in_set(RenderSet::PrepareBindGroups),
            );

        // Add Render Graph nodes / 添加 Render Graph 节点
        render_app
            .add_render_graph_node::<FogComputeNode>(Core2d, FogComputeNodeLabel)
            .add_render_graph_node::<ViewNodeRunner<FogOverlayNode>>(Core2d, FogOverlayNodeLabel);

        // Add Render Graph edges (define dependencies) / 添加 Render Graph 边 (定义依赖)
        render_app.add_render_graph_edges(
            Core2d,
            (
                Node2d::MainTransparentPass,
                FogComputeNodeLabel,
                FogOverlayNodeLabel,
                Node2d::EndMainPass,
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
            .init_resource::<compute::FogComputePipeline>()
            .init_resource::<overlay::FogOverlayPipeline>();
    }
}
