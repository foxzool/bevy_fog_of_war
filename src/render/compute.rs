//! GPU compute shader pipeline for fog of war visibility calculations.
//! GPU计算着色器管线用于战争迷雾可见性计算
//!
//! This module implements the core GPU compute pipeline that calculates fog of war
//! visibility in real-time using GPU compute shaders. It processes multiple chunks
//! in parallel and applies vision source calculations to update fog textures.
//!
//! # Pipeline Architecture
//!
//! ## GPU Compute Approach
//! The fog system uses GPU compute shaders for high-performance visibility calculations:
//! - **Parallel Processing**: Multiple chunks processed simultaneously
//! - **Texture Operations**: Direct texture array read/write operations
//! - **Vision Algorithms**: Implements circle, cone, and square vision shapes
//! - **Real-time Updates**: Executes every frame for dynamic fog updates
//!
//! ## Compute Shader Integration
//! ```text
//! [Vision Sources] → [GPU Buffers] → [Compute Shader] → [Fog Textures]
//!        ↓               ↓              ↓                ↓
//!   Entity Data    Bind Groups     WGSL Kernel      Texture Arrays
//!   Chunk Info     GPU Memory      GPU Execution    Updated Fog
//! ```
//!
//! ## Workgroup Organization
//! - **Workgroup Size**: 8x8 threads per workgroup (64 threads total)
//! - **Texture Coverage**: Each thread processes one texture pixel
//! - **Chunk Processing**: Z-dimension dispatches per chunk
//! - **Scalability**: Workgroups scale with texture resolution and chunk count
//!
//! # Performance Characteristics
//!
//! ## GPU Utilization
//! - **Compute Units**: Utilizes GPU compute capabilities for parallel processing
//! - **Memory Bandwidth**: Efficient texture array access patterns
//! - **Thread Efficiency**: Optimized workgroup size for GPU architecture
//! - **Scalability**: O(chunks × texture_size) but parallelized on GPU
//!
//! ## Memory Access Patterns
//! - **Texture Arrays**: Read/write access to visibility and fog texture arrays
//! - **Uniform Data**: Shared settings data across all compute threads
//! - **Storage Buffers**: Vision source and chunk data for compute kernels
//! - **Coalesced Access**: Optimized memory access patterns for GPU performance
//!
//! ## Dispatch Calculation
//! ```text
//! workgroups_x = texture_resolution.x.div_ceil(8)  // X-axis workgroups
//! workgroups_y = texture_resolution.y.div_ceil(8)  // Y-axis workgroups
//! workgroups_z = active_chunk_count               // Z-axis per chunk
//! total_threads = workgroups_x * workgroups_y * workgroups_z * 64
//! ```
//!
//! # Shader Resources
//!
//! ## Bind Group Layout (Binding Index)
//! - **0**: Visibility texture array (R8Unorm, ReadWrite) - Real-time visibility data
//! - **1**: Fog texture array (R8Unorm, WriteOnly) - Persistent exploration data
//! - **2**: Vision source buffer (Storage, ReadOnly) - Vision source parameters
//! - **3**: Chunk compute buffer (Storage, ReadOnly) - Chunk metadata
//! - **4**: Fog settings uniform (Uniform, ReadOnly) - Global fog configuration
//!
//! ## Texture Format Details
//! - **R8Unorm**: Single-channel 8-bit normalized format (0.0-1.0 range)
//! - **Array Layers**: Multiple chunks stored in texture array layers
//! - **Resolution**: Configurable texture resolution per chunk
//! - **Memory Efficiency**: Compact format optimized for fog data
//!
//! # Integration Points
//!
//! ## Render Graph Position
//! The compute node executes between main rendering and fog overlay:
//! ```text
//! MainTransparentPass → SnapshotNode → FogComputeNode → FogOverlayNode
//! ```
//!
//! ## System Dependencies
//! - **Extract Systems**: Provide vision source and chunk data
//! - **Prepare Systems**: Create GPU buffers and bind groups
//! - **Overlay System**: Consumes computed fog textures for rendering
//! - **Transfer Systems**: Handle CPU↔GPU memory transfers
//!
//! # Error Handling
//!
//! ## Graceful Degradation
//! The system handles missing resources gracefully:
//! - **Pipeline Compilation**: Waits for shader compilation without blocking
//! - **Bind Group Availability**: Skips execution when resources not ready
//! - **Empty Workload**: Efficiently handles zero chunks without GPU work
//! - **Camera Filtering**: Avoids compute work for snapshot cameras
//!
//! # Future Optimizations
//!
//! ## Potential Enhancements
//! - **LOD System**: Different resolution per distance from camera
//! - **Frustum Culling**: Skip chunks outside camera view
//! - **Temporal Coherence**: Incremental updates for static areas
//! - **GPU Culling**: GPU-side chunk visibility culling

use super::prepare::{FogBindGroups, GpuChunkInfoBuffer};
use crate::render::extract::{ChunkComputeData, RenderFogMapSettings, VisionSourceData};
use crate::snapshot::SnapshotCamera;
use bevy_asset::DirectAssetAccessExt;
use bevy_ecs::prelude::*;
use bevy_render::{
    render_graph::{Node, NodeRunError, RenderGraphContext, RenderLabel},
    render_resource::{
        BindGroupLayout, BindGroupLayoutEntries, CachedComputePipelineId, ComputePassDescriptor,
        ComputePipelineDescriptor, PipelineCache, ShaderStages,
        StorageTextureAccess::{ReadWrite, WriteOnly},
        TextureFormat,
        binding_types::{storage_buffer_read_only, texture_storage_2d_array, uniform_buffer},
    },
    renderer::{RenderContext, RenderDevice},
};

/// Path to the WGSL compute shader file that implements fog visibility calculations.
/// 实现雾效可见性计算的WGSL计算着色器文件路径
///
/// This shader contains the core fog of war algorithms implemented in WGSL
/// (WebGPU Shading Language). The shader processes vision sources and updates
/// fog textures based on line-of-sight calculations and vision shapes.
///
/// # Shader Capabilities
/// - **Vision Shapes**: Circle, cone, and square vision implementations
/// - **Line-of-Sight**: Raycasting algorithms for vision blocking
/// - **Texture Updates**: Direct texture array read/write operations
/// - **Parallel Processing**: Optimized for GPU parallel execution
///
/// # Asset Loading
/// The shader is loaded as a Bevy asset and compiled into the compute pipeline
/// during application startup. Compilation errors will prevent fog rendering.
const SHADER_ASSET_PATH: &str = "shaders/fog_compute.wgsl";

/// Render graph label for the fog compute shader node.
/// 雾效计算着色器节点的渲染图标签
///
/// This label identifies the fog compute node within Bevy's render graph,
/// enabling proper ordering and dependency management between render passes.
/// The compute node executes after scene rendering but before fog overlay.
///
/// # Render Graph Integration
/// Used to establish render graph dependencies:
/// ```rust,ignore
/// render_app.add_render_graph_edges(
///     Core2d,
///     (
///         Node2d::MainTransparentPass,
///         SnapshotNodeLabel,
///         FogComputeNodeLabel,  // This label
///         FogOverlayNodeLabel,
///         Node2d::EndMainPass,
///     ),
/// );
/// ```
///
/// # Label Properties
/// - **Unique**: Distinguishes compute node from other render nodes
/// - **Hashable**: Enables efficient render graph operations
/// - **Debug**: Provides debugging information for render graph inspection
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct FogComputeNodeLabel;

/// Render graph node that executes the fog of war compute shader.
/// 执行战争迷雾计算着色器的渲染图节点
///
/// This node manages the execution of GPU compute shaders for fog visibility
/// calculations. It sets up compute passes, binds resources, and dispatches
/// workgroups to process fog updates across all active chunks.
///
/// # Execution Process
/// 1. **Resource Validation**: Check pipeline and bind group availability
/// 2. **Workgroup Calculation**: Determine dispatch dimensions based on chunks
/// 3. **Compute Pass Setup**: Create compute pass with proper descriptors
/// 4. **Pipeline Binding**: Bind compute pipeline and resource groups
/// 5. **Workgroup Dispatch**: Execute compute shader across all chunks
///
/// # Performance Optimization
/// - **Early Exit**: Skips execution when no work is needed
/// - **Batch Processing**: Processes all chunks in single compute pass
/// - **Resource Reuse**: Reuses pipeline and bind groups across frames
/// - **Efficient Dispatch**: Optimizes workgroup dimensions for GPU utilization
///
/// # Error Resilience
/// - **Missing Pipeline**: Gracefully handles uncompiled shaders
/// - **Resource Unavailability**: Skips execution when bind groups not ready
/// - **Zero Work**: Efficiently handles empty chunk sets
/// - **Camera Filtering**: Avoids unnecessary work for snapshot cameras
#[derive(Default)]
pub struct FogComputeNode;

/// GPU compute pipeline resource for fog of war visibility calculations.
/// 战争迷雾可见性计算的GPU计算管线资源
///
/// This resource encapsulates the compiled GPU compute pipeline and associated
/// bind group layout for fog calculations. It's created once during application
/// startup and reused for all fog compute operations.
///
/// # Pipeline Components
/// - **Compute Shader**: WGSL shader implementing fog visibility algorithms
/// - **Bind Group Layout**: Resource binding schema for compute operations
/// - **Pipeline State**: Compiled GPU pipeline state object
/// - **Resource Binding**: Defines how CPU resources map to GPU shader inputs
///
/// # Resource Binding Schema
/// The pipeline expects resources bound in this specific order:
/// ```wgsl
/// @group(0) @binding(0) var visibility_texture: texture_storage_2d_array<r8unorm, read_write>;
/// @group(0) @binding(1) var fog_texture: texture_storage_2d_array<r8unorm, write>;
/// @group(0) @binding(2) var<storage, read> vision_sources: array<VisionSourceData>;
/// @group(0) @binding(3) var<storage, read> chunks: array<ChunkComputeData>;
/// @group(0) @binding(4) var<uniform> settings: RenderFogMapSettings;
/// ```
///
/// # Performance Characteristics
/// - **Compilation Cost**: One-time shader compilation during startup
/// - **Memory Overhead**: Minimal pipeline state storage
/// - **Reusability**: Single pipeline used for all fog compute operations
/// - **GPU Efficiency**: Optimized bind group layout for memory access
///
/// # Shader Integration
/// The pipeline interfaces with the fog compute shader which implements:
/// - **Vision Shape Algorithms**: Circle, cone, and square vision calculations
/// - **Line-of-Sight**: Raycasting for vision blocking obstacles
/// - **Texture Updates**: Direct fog texture modification
/// - **Parallel Processing**: Multi-threaded GPU execution
#[derive(Resource)]
pub struct FogComputePipeline {
    /// Cached compute pipeline identifier for efficient pipeline retrieval.
    /// 用于高效管线检索的缓存计算管线标识符
    ///
    /// This ID is used with Bevy's pipeline cache to retrieve the compiled
    /// compute pipeline. The pipeline may not be immediately available if
    /// shader compilation is still in progress.
    pub pipeline_id: CachedComputePipelineId,

    /// Bind group layout defining resource binding schema for the compute shader.
    /// 定义计算着色器资源绑定模式的绑定组布局
    ///
    /// This layout specifies how CPU resources (textures, buffers, uniforms)
    /// are bound to GPU shader inputs. It's used to create bind groups that
    /// provide data to the compute shader during execution.
    pub compute_layout: BindGroupLayout,
}

/// Initializes the fog compute pipeline from world resources during application startup.
/// 在应用程序启动期间从世界资源初始化雾效计算管线
///
/// This implementation creates the complete GPU compute pipeline including shader
/// compilation, bind group layout creation, and pipeline state object setup.
/// It's called once during application initialization to prepare the fog system.
///
/// # Initialization Process
/// 1. **Device Access**: Retrieve render device for GPU resource creation
/// 2. **Layout Creation**: Define bind group layout for shader resource binding
/// 3. **Shader Loading**: Load WGSL compute shader as Bevy asset
/// 4. **Pipeline Queuing**: Queue compute pipeline for compilation
/// 5. **Resource Storage**: Store pipeline ID and layout for runtime use
///
/// # Bind Group Layout Structure
/// Creates a sequential binding layout with 5 bindings:
/// - **Binding 0**: Visibility texture array (R8Unorm, ReadWrite)
/// - **Binding 1**: Fog texture array (R8Unorm, WriteOnly)
/// - **Binding 2**: Vision source storage buffer (ReadOnly)
/// - **Binding 3**: Chunk compute data storage buffer (ReadOnly)
/// - **Binding 4**: Fog settings uniform buffer (ReadOnly)
///
/// # Shader Compilation
/// The pipeline descriptor specifies:
/// - **Entry Point**: "main" function in the WGSL shader
/// - **Shader Defs**: Empty (no conditional compilation)
/// - **Push Constants**: None (all data via bind groups)
/// - **Workgroup Memory**: Not zero-initialized for performance
///
/// # Performance Considerations
/// - **One-time Cost**: Expensive operation performed only at startup
/// - **GPU Validation**: GPU driver validates shader and pipeline state
/// - **Memory Allocation**: Allocates GPU pipeline state objects
/// - **Compilation Time**: Shader compilation may take several milliseconds
///
/// # Error Handling
/// Pipeline creation failures would panic during application startup,
/// providing early feedback about GPU compatibility or shader issues.
impl FromWorld for FogComputePipeline {
    /// Creates a new fog compute pipeline from world resources.
    /// 从世界资源创建新的雾效计算管线
    ///
    /// This method is called by Bevy's resource initialization system to create
    /// the compute pipeline. It accesses necessary world resources and sets up
    /// the complete GPU pipeline for fog calculations.
    ///
    /// # Resource Dependencies
    /// Requires these world resources to be available:
    /// - **PipelineCache**: For compute pipeline compilation and caching
    /// - **RenderDevice**: For GPU resource creation and bind group layouts
    /// - **Asset System**: For loading the WGSL compute shader file
    ///
    /// # GPU Resource Creation
    /// Creates several GPU resources:
    /// - **Bind Group Layout**: Defines shader resource binding schema
    /// - **Pipeline Descriptor**: Specifies compute pipeline configuration
    /// - **Shader Asset**: Loads WGSL compute shader from file system
    ///
    /// # Return Value
    /// Returns a configured FogComputePipeline with:
    /// - **pipeline_id**: Cached pipeline ID for runtime retrieval
    /// - **compute_layout**: Bind group layout for resource binding
    ///
    /// # Time Complexity
    /// O(1) for resource access and pipeline queuing, but shader compilation
    /// happens asynchronously and may take additional time.
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

/// Implements the render graph node interface for fog compute operations.
/// 为雾效计算操作实现渲染图节点接口
///
/// This implementation defines how the fog compute node executes within Bevy's
/// render graph. It handles resource validation, compute pass setup, and
/// workgroup dispatch for fog visibility calculations.
impl Node for FogComputeNode {
    /// Executes the fog compute shader to update visibility and fog textures.
    /// 执行雾效计算着色器以更新可见性和雾效纹理
    ///
    /// This method is called by Bevy's render graph system to execute fog
    /// calculations. It validates resources, sets up compute passes, and
    /// dispatches GPU workgroups to process all active fog chunks.
    ///
    /// # Execution Flow
    /// 1. **Camera Validation**: Skip execution for snapshot cameras
    /// 2. **Resource Gathering**: Retrieve pipeline, bind groups, and settings
    /// 3. **Pipeline Validation**: Ensure compute pipeline is compiled
    /// 4. **Bind Group Validation**: Verify compute bind group is ready
    /// 5. **Work Calculation**: Determine workgroup dispatch dimensions
    /// 6. **Compute Pass**: Create and execute GPU compute pass
    /// 7. **Workgroup Dispatch**: Launch compute shader across all chunks
    ///
    /// # Workgroup Dispatch Calculation
    /// ```rust,ignore
    /// // Each workgroup covers 8x8 texture pixels
    /// workgroups_x = texture_resolution.x.div_ceil(8)
    /// workgroups_y = texture_resolution.y.div_ceil(8)
    /// workgroups_z = active_chunk_count
    ///
    /// // Total threads = workgroups_x × workgroups_y × workgroups_z × 64
    /// ```
    ///
    /// # Performance Optimization
    /// - **Early Exit**: Skips execution when no work is needed
    /// - **Resource Validation**: Avoids GPU work when resources unavailable
    /// - **Efficient Dispatch**: Optimizes workgroup dimensions for coverage
    /// - **Single Pass**: Processes all chunks in one compute pass
    ///
    /// # Error Handling
    /// Returns Ok(()) in all cases to maintain render graph stability:
    /// - **Snapshot Camera**: Not applicable for fog compute
    /// - **Missing Pipeline**: Shader still compiling, skip this frame
    /// - **Missing Bind Group**: Resources not ready, skip this frame
    /// - **Zero Chunks**: No work to do, skip efficiently
    ///
    /// # GPU Memory Access
    /// The compute shader accesses:
    /// - **Visibility Textures**: Read/write real-time visibility data
    /// - **Fog Textures**: Write persistent exploration data
    /// - **Vision Data**: Read vision source parameters
    /// - **Chunk Data**: Read chunk metadata and transformations
    /// - **Settings**: Read global fog configuration
    ///
    /// # Integration Points
    /// - **Render Graph**: Executes between scene rendering and fog overlay
    /// - **Pipeline Cache**: Retrieves compiled compute pipeline
    /// - **Bind Groups**: Uses prepared GPU resource bindings
    /// - **Chunk System**: Processes all active GPU-resident chunks
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
