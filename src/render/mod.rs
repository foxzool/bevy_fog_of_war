//! Fog of war rendering subsystem for GPU-accelerated fog calculations and visual overlay.
//! 战争迷雾渲染子系统，用于GPU加速的雾效计算和视觉叠加
//!
//! This module implements the complete rendering pipeline for fog of war using Bevy's
//! render framework. It coordinates GPU compute shaders, texture management, and
//! overlay rendering to provide high-performance fog of war visualization.
//!
//! # Render Pipeline Architecture
//!
//! ## Data Flow Overview
//! ```text
//! [Main World] → Extract → [Render World] → Prepare → Compute → Overlay → [Screen]
//!      ↓              ↓          ↓           ↓         ↓         ↓
//! Vision Sources → GPU Buffers → Uniforms → Compute → Textures → Final Image
//! Chunk Data    → Bind Groups → Barriers  → Shaders → Overlay  → Composition
//! Settings      → Resources   → Commands  → Results → Blend    → Display
//! ```
//!
//! ## Render Graph Integration
//! The fog of war system integrates into Bevy's Core2d render graph:
//! ```text
//! MainTransparentPass → FogComputeNode → FogOverlayNode → EndMainPass
//! ```
//!
//! ## System Organization
//!
//! ### Extract Systems (Main → Render World)
//! - **extract_fog_settings**: Transfers FogMapSettings configuration
//! - **extract_vision_sources**: Copies VisionSource entity data
//! - **extract_gpu_chunk_data**: Transfers chunk state and texture information
//! - **extract_texture_handles**: Synchronizes GPU texture array handles
//!
//! ### Prepare Systems (GPU Resource Setup)
//! - **prepare_fog_uniforms**: Creates/updates uniform buffers with settings
//! - **prepare_vision_source_buffer**: Builds GPU buffer of vision source data
//! - **prepare_gpu_chunk_buffer**: Prepares chunk metadata for compute shaders
//! - **prepare_overlay_chunk_mapping**: Sets up chunk-to-layer mapping data
//! - **prepare_fog_bind_groups**: Creates GPU bind groups for shader access
//!
//! ### Compute Systems (GPU Calculation)
//! - **FogComputeNode**: Executes GPU compute shaders for fog visibility calculations
//! - Uses texture arrays for input/output, processes multiple chunks in parallel
//! - Implements vision algorithms (circle, cone, square) on GPU for performance
//!
//! ### Overlay Systems (Visual Composition)
//! - **FogOverlayNode**: Renders final fog overlay onto the main camera view
//! - Blends fog textures with snapshot textures for visual continuity
//! - Supports configurable fog colors, transparency, and blending modes
//!
//! ### Transfer Systems (CPU↔GPU Memory)
//! - **CPU→GPU**: Uploads chunk texture data from main memory to GPU arrays
//! - **GPU→CPU**: Downloads GPU texture data for persistence and memory management
//! - **Async Processing**: Handles asynchronous buffer mapping and data transfer
//!
//! # Performance Characteristics
//! - **GPU Compute**: Parallel processing of visibility calculations
//! - **Memory Efficiency**: Texture arrays minimize GPU memory fragmentation
//! - **Async Transfers**: Non-blocking CPU↔GPU data movement
//! - **Scalability**: O(chunks) compute complexity, supports large worlds
//!
//! # Submodules
//! - **compute**: GPU compute shader pipeline for fog calculations
//! - **extract**: Main world to render world data extraction
//! - **overlay**: Final fog overlay rendering and composition
//! - **prepare**: GPU resource preparation and bind group management
//! - **transfer**: CPU↔GPU memory transfer coordination

use bevy_core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy_render::{Render, RenderApp, RenderSet};
use bevy_render::render_graph::{RenderGraphApp, ViewNodeRunner};
use bevy_render::renderer::render_system;
use crate::prelude::*;

// Render pipeline submodules
// 渲染管线子模块
mod compute; // GPU compute shader pipeline for fog calculations / GPU计算着色器管线用于雾效计算
mod extract; // Main world to render world data extraction / 主世界到渲染世界的数据提取
mod overlay; // Final fog overlay rendering and composition / 最终雾效叠加渲染和合成
mod prepare; // GPU resource preparation and bind group management / GPU资源准备和绑定组管理
mod transfer; // CPU↔GPU memory transfer coordination / CPU↔GPU内存传输协调

// Internal module imports for transfer system coordination
// 用于传输系统协调的内部模块导入
use crate::render::transfer::{CpuToGpuRequests, GpuToCpuActiveCopies};

// Public re-exports of render system components
// 渲染系统组件的公共重新导出

// Compute shader pipeline components
// 计算着色器管线组件
pub use compute::{FogComputeNode, FogComputeNodeLabel};

// Extracted render world resources
// 提取的渲染世界资源
pub use extract::{RenderFogMapSettings, RenderSnapshotTempTexture, RenderSnapshotTexture};

// Fog overlay rendering components
// 雾效叠加渲染组件
pub use overlay::{FogOverlayNode, FogOverlayNodeLabel};

// GPU resource management components
// GPU资源管理组件
pub use prepare::{
    FogBindGroups, FogUniforms, GpuChunkInfoBuffer, OverlayChunkMappingBuffer, VisionSourceBuffer,
};

/// Plugin that configures the complete fog of war rendering pipeline in Bevy's render world.
/// 在Bevy渲染世界中配置完整战争迷雾渲染管线的插件
///
/// This plugin is automatically registered by the main FogOfWarPlugin and handles all
/// GPU-side operations for fog of war. It sets up the render graph nodes, systems,
/// and resources necessary for high-performance fog calculation and visualization.
///
/// # Architecture Integration
/// - **Render App**: Operates exclusively within Bevy's render world
/// - **System Scheduling**: Coordinates with Bevy's RenderSet scheduling
/// - **Render Graph**: Integrates fog nodes into Core2d render graph
/// - **Resource Management**: Manages GPU buffers, textures, and bind groups
///
/// # Performance Benefits
/// - **GPU Computing**: Leverages compute shaders for parallel fog calculations
/// - **Memory Efficiency**: Uses texture arrays to minimize GPU memory overhead
/// - **Pipeline Integration**: Seamlessly fits into Bevy's rendering architecture
/// - **Asynchronous Operations**: Non-blocking CPU↔GPU data transfers
///
/// # System Dependencies
/// The plugin establishes proper ordering dependencies:
/// ```text
/// Extract → Prepare → Compute → Overlay → Transfer
/// ```
/// This ensures data flows correctly through the render pipeline without race conditions.
pub struct FogOfWarRenderPlugin;

impl Plugin for FogOfWarRenderPlugin {
    /// Configures the render world with fog of war systems, resources, and render graph integration.
    /// 使用战争迷雾系统、资源和渲染图集成配置渲染世界
    ///
    /// # System Configuration
    /// Sets up three main categories of render systems:
    ///
    /// ## 1. Extract Systems (ExtractSchedule)
    /// Transfer data from main world to render world:
    /// - **Settings Extraction**: Copies FogMapSettings configuration
    /// - **Vision Source Extraction**: Transfers entity data for GPU processing
    /// - **Chunk Data Extraction**: Copies chunk states and texture indices
    /// - **Texture Handle Extraction**: Synchronizes GPU texture array handles
    /// - **Transfer Processing**: Handles CPU↔GPU memory operations
    ///
    /// ## 2. Prepare Systems (RenderSet::PrepareBindGroups)
    /// Create and update GPU resources:
    /// - **Uniform Buffers**: Global settings and configuration data
    /// - **Vision Source Buffer**: GPU buffer containing vision source parameters
    /// - **Chunk Info Buffer**: Metadata for active chunks on GPU
    /// - **Overlay Mapping Buffer**: Chunk-to-texture-layer mapping information
    /// - **Bind Groups**: GPU resource bindings for compute and render shaders
    ///
    /// ## 3. Transfer Systems (RenderSet::PrepareResources & Render)
    /// Coordinate CPU↔GPU memory transfers:
    /// - **CPU→GPU Uploads**: Transfer chunk data from main memory to GPU
    /// - **GPU→CPU Downloads**: Extract GPU texture data for persistence
    /// - **Async Buffer Mapping**: Handle asynchronous GPU buffer operations
    /// - **Reset Operations**: Clear textures during fog reset operations
    ///
    /// # Render Graph Integration
    /// Adds fog nodes to Core2d render graph with proper dependencies:
    /// ```text
    /// MainTransparentPass → FogComputeNode → FogOverlayNode → EndMainPass
    /// ```
    ///
    /// # Performance Considerations
    /// - **System Ordering**: Ensures proper data flow without race conditions
    /// - **Memory Management**: Initializes GPU resources efficiently
    /// - **Async Operations**: Uses proper scheduling for asynchronous transfers
    /// - **Resource Reuse**: Shares GPU resources across multiple systems
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
                    transfer::check_and_clear_textures_on_reset,
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

    /// Finalizes render pipeline initialization by creating GPU pipelines and render states.
    /// 通过创建GPU管线和渲染状态来完成渲染管线初始化
    ///
    /// This method is called during Bevy's plugin finalization phase after all plugins
    /// have been registered. It initializes the GPU pipelines that require access to
    /// the render device and other finalized render resources.
    ///
    /// # Pipeline Initialization
    /// - **FogComputePipeline**: Creates compute shader pipeline for fog calculations
    /// - **FogOverlayPipeline**: Creates render pipeline for fog overlay composition
    ///
    /// # Why finish() is needed
    /// GPU pipelines require the render device to be fully initialized, which happens
    /// after the build() phase. The finish() phase ensures all render resources are
    /// available before pipeline creation.
    ///
    /// # Performance Impact
    /// - **One-time Cost**: Pipeline creation occurs once during app initialization
    /// - **GPU Resources**: Allocates shader programs and pipeline state objects
    /// - **Memory Usage**: Minimal overhead for pipeline state storage
    /// - **Validation**: GPU drivers validate shader programs during creation
    ///
    /// # Error Handling
    /// Pipeline creation failures would panic during app initialization, providing
    /// early feedback about GPU compatibility or shader compilation issues.
    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<compute::FogComputePipeline>()
            .init_resource::<overlay::FogOverlayPipeline>();
    }
}
