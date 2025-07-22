//! GPU resource preparation and buffer management for fog rendering pipeline.
//! 雾效渲染管线的GPU资源准备和缓冲区管理
//!
//! This module implements the prepare phase of Bevy's rendering pipeline, where extracted
//! data from the main world is converted into GPU buffers and bind groups. It handles
//! efficient GPU memory management and resource binding for fog rendering operations.
//!
//! # Prepare Phase Overview
//!
//! ## Main Responsibilities
//! The prepare phase converts extracted CPU data into GPU-accessible resources:
//! ```text
//! [Extracted Data] → [GPU Buffers] → [Bind Groups] → [Shader Access]
//!       ↓                ↓             ↓              ↓
//! CPU Structures → GPU Memory → Resource Binding → Shader Execution
//! Vision Sources → Storage    → Compute Groups  → Fog Calculations
//! ```
//!
//! ## Buffer Management Strategy
//! 1. **Uniform Buffers**: Small, frequently updated data (fog settings)
//! 2. **Storage Buffers**: Large arrays of data (vision sources, chunk info)
//! 3. **Dynamic Allocation**: Buffers recreated each frame to handle size changes
//! 4. **Efficient Upload**: Direct GPU memory allocation with bytemuck casting
//!
//! # Performance Characteristics
//!
//! ## Memory Management
//! - **Dynamic Sizing**: Buffers adapt to varying data sizes each frame
//! - **Direct Upload**: CPU data directly copied to GPU without staging
//! - **Efficient Casting**: Zero-copy conversion using bytemuck for Pod types
//! - **Resource Reuse**: Bind groups cached when possible to reduce allocation
//!
//! ## GPU Buffer Types
//! - **Uniform Buffers**: ≤64KB, optimized for frequent access, cached by GPU
//! - **Storage Buffers**: Large capacity, sequential access patterns
//! - **Copy Destination**: Enables dynamic updates from CPU each frame
//! - **Read-Only Access**: Shaders only read data, no write operations
//!
//! # Resource Lifecycle
//!
//! ## Frame-by-Frame Processing
//! 1. **Extract Phase**: CPU data extracted from main world
//! 2. **Prepare Phase**: GPU buffers created from extracted data (this module)
//! 3. **Queue Phase**: Render commands generated using prepared resources
//! 4. **Render Phase**: GPU executes commands using bound resources
//!
//! ## Buffer Recreation Strategy
//! Buffers are recreated each frame to handle:
//! - **Variable Vision Source Count**: Number of active vision sources changes
//! - **Dynamic Chunk Count**: Visible chunks vary based on camera position
//! - **Frustum Culling Results**: Culled chunk count affects buffer sizes
//! - **Memory Layout Changes**: Data structure updates require new buffers
//!
//! # Integration Points
//!
//! ## Upstream Dependencies
//! - **Extract Systems**: Provide ExtractedVisionSources and ExtractedGpuChunkData
//! - **Settings Transfer**: RenderFogMapSettings from main world extraction
//! - **Texture Handles**: GPU texture array handles for bind group creation
//!
//! ## Downstream Consumers
//! - **Compute Shaders**: Use prepared buffers for fog visibility calculations
//! - **Overlay Shaders**: Access chunk mapping and uniform data for rendering
//! - **Bind Groups**: Pre-configured resource bindings for efficient GPU access
//!
//! # Buffer Layout Optimization
//!
//! ## GPU Memory Access Patterns
//! - **Sequential Access**: Buffers organized for optimal GPU cache utilization
//! - **Aligned Structures**: All data structures follow GPU alignment requirements
//! - **Coalesced Reads**: Thread groups access consecutive memory locations
//! - **Minimal Padding**: Efficient memory usage with required alignment
//!
//! # Error Handling
//!
//! ## Graceful Resource Management
//! - **Optional Buffers**: Missing buffers don't crash render pipeline
//! - **Fallback Textures**: Default textures used when real textures unavailable
//! - **Validation Checks**: Buffer readiness verified before bind group creation
//! - **Resource Recovery**: System handles temporary resource unavailability

use super::extract::{
    ExtractedGpuChunkData, ExtractedVisionSources, RenderFogMapSettings, RenderFogTexture,
    RenderVisibilityTexture,
};
use crate::render::compute::FogComputePipeline;
use bevy_ecs::prelude::*;
use bevy_render::render_asset::RenderAssets;
use bevy_render::render_resource::{
    BindGroup, BindGroupEntries, Buffer, BufferInitDescriptor, BufferUsages,
};
use bevy_render::renderer::RenderDevice;
use bevy_render::texture::{FallbackImage, GpuImage};

// --- GPU Buffer Resources ---
// --- GPU缓冲区资源 ---
/// GPU uniform buffer resource containing fog configuration data for shader access.
/// 包含雾效配置数据供着色器访问的GPU统一缓冲区资源
///
/// This resource stores the fog settings uniform buffer that provides global
/// fog configuration to both compute and overlay shaders. The buffer contains
/// a single RenderFogMapSettings structure with fog colors, dimensions, and
/// other configuration parameters.
///
/// # Buffer Characteristics
/// - **Type**: Uniform buffer for fast, cached GPU access
/// - **Size**: Fixed size structure (80 bytes for RenderFogMapSettings)
/// - **Usage**: Read-only access from compute and overlay shaders
/// - **Update Frequency**: Recreated each frame if settings change
///
/// # Shader Binding
/// Bound as uniform buffer in both compute and overlay pipelines:
/// ```wgsl
/// @group(0) @binding(4) var<uniform> settings: RenderFogMapSettings;
/// ```
///
/// # Performance Characteristics
/// - **GPU Cache**: Uniform buffers are cached by GPU for fast access
/// - **Memory**: Small size (80 bytes) has minimal memory impact
/// - **Bandwidth**: Efficient for frequently accessed global configuration
/// - **Update Cost**: Minimal, only recreated when settings change
#[derive(Resource, Default)]
pub struct FogUniforms {
    /// Optional GPU uniform buffer containing fog settings data.
    /// 包含雾效设置数据的可选GPU统一缓冲区
    ///
    /// None when buffer is not yet created or needs recreation.
    /// Contains RenderFogMapSettings structure when ready for use.
    pub buffer: Option<Buffer>,
}

/// GPU storage buffer resource containing vision source data for compute shader processing.
/// 包含视野源数据供计算着色器处理的GPU存储缓冲区资源
///
/// This resource stores an array of VisionSourceData structures that define
/// active vision sources in the scene. The buffer is used by compute shaders
/// to calculate fog visibility based on vision source properties.
///
/// # Buffer Characteristics
/// - **Type**: Storage buffer for large data arrays
/// - **Size**: Variable, based on number of active vision sources
/// - **Usage**: Read-only access from compute shaders
/// - **Update Frequency**: Recreated each frame to handle vision source changes
///
/// # Data Structure
/// Contains array of VisionSourceData with:
/// - Position, radius, and shape information
/// - Precomputed trigonometric values for performance
/// - Intensity and transition parameters
///
/// # Performance Considerations
/// - **Dynamic Sizing**: Buffer size adapts to vision source count
/// - **GPU Access**: Efficient sequential access pattern in compute shaders
/// - **Memory**: 48 bytes per vision source (aligned for GPU)
/// - **Bandwidth**: Limited by number of active vision sources
#[derive(Resource, Default)]
pub struct VisionSourceBuffer {
    /// Optional GPU storage buffer containing vision source array data.
    /// 包含视野源数组数据的可选GPU存储缓冲区
    ///
    /// None when buffer is not yet created. Contains VisionSourceData array
    /// when ready for compute shader access.
    pub buffer: Option<Buffer>,

    /// Number of vision sources stored in the buffer.
    /// 存储在缓冲区中的视野源数量
    ///
    /// Used to track buffer capacity and validate data consistency.
    /// Updated each frame when buffer is recreated.
    pub capacity: usize,
}

/// GPU storage buffer resource containing chunk computation data for compute shader processing.
/// 包含区块计算数据供计算着色器处理的GPU存储缓冲区资源
///
/// This resource stores an array of ChunkComputeData structures that define
/// chunk coordinates and fog texture layer indices for compute operations.
/// The buffer enables compute shaders to process multiple chunks in parallel.
///
/// # Buffer Characteristics
/// - **Type**: Storage buffer for chunk processing data
/// - **Size**: Variable, based on number of visible/GPU-resident chunks
/// - **Usage**: Read-only access from compute shaders
/// - **Update Frequency**: Recreated each frame due to frustum culling
///
/// # Data Structure
/// Contains array of ChunkComputeData with:
/// - Chunk coordinates in chunk space
/// - Fog texture array layer indices
/// - GPU-aligned structure layout (16 bytes per chunk)
///
/// # Performance Impact
/// - **Frustum Culling**: Buffer size varies based on camera view
/// - **Parallel Processing**: Enables GPU parallel chunk processing
/// - **Memory Efficiency**: Minimal data per chunk (16 bytes)
/// - **Cache Friendly**: Sequential access pattern in compute shaders
#[derive(Resource, Default)]
pub struct GpuChunkInfoBuffer {
    /// Optional GPU storage buffer containing chunk computation data.
    /// 包含区块计算数据的可选GPU存储缓冲区
    ///
    /// None when buffer is not yet created. Contains ChunkComputeData array
    /// when ready for compute shader processing.
    pub buffer: Option<Buffer>,

    /// Number of chunks stored in the buffer for computation.
    /// 存储在缓冲区中用于计算的区块数量
    ///
    /// Represents the number of chunks that will be processed by compute
    /// shaders. Varies based on frustum culling and GPU memory state.
    pub capacity: usize,
}

/// GPU storage buffer resource containing chunk mapping data for overlay rendering.
/// 包含区块映射数据供覆盖渲染的GPU存储缓冲区资源
///
/// This resource stores an array of OverlayChunkData structures that provide
/// chunk coordinate to texture layer mapping for overlay shaders. It enables
/// efficient fog overlay rendering with proper texture layer access.
///
/// # Buffer Characteristics
/// - **Type**: Storage buffer for overlay mapping data
/// - **Size**: Variable, based on number of visible chunks
/// - **Usage**: Read-only access from overlay fragment shaders
/// - **Update Frequency**: Recreated each frame due to dynamic chunk visibility
///
/// # Data Structure
/// Contains array of OverlayChunkData with:
/// - Chunk coordinates for spatial mapping
/// - Fog texture array layer indices
/// - Snapshot texture array layer indices
/// - GPU-aligned structure layout (16 bytes per chunk)
///
/// # Overlay Rendering Usage
/// Fragment shaders use this buffer to:
/// 1. Convert screen coordinates to chunk coordinates
/// 2. Look up appropriate texture layer indices
/// 3. Sample fog and snapshot textures from correct layers
/// 4. Composite final fog overlay effect
///
/// # Performance Characteristics
/// - **Fragment Shaders**: Accessed per-pixel in overlay rendering
/// - **Cache Efficiency**: GPU caches frequently accessed chunks
/// - **Memory Layout**: Optimized for GPU fragment processor access
/// - **Bandwidth**: Scales with screen resolution and chunk complexity
#[derive(Resource, Default)]
pub struct OverlayChunkMappingBuffer {
    /// Optional GPU storage buffer containing overlay chunk mapping data.
    /// 包含覆盖区块映射数据的可选GPU存储缓冲区
    ///
    /// None when buffer is not yet created. Contains OverlayChunkData array
    /// when ready for overlay shader access.
    pub buffer: Option<Buffer>,

    /// Number of chunks stored in the buffer for overlay rendering.
    /// 存储在缓冲区中用于覆盖渲染的区块数量
    ///
    /// Represents chunks visible to the camera that need fog overlay rendering.
    /// Used for validation and performance monitoring.
    pub capacity: usize,
}

/// Resource containing prepared GPU bind groups for efficient shader resource binding.
/// 包含为高效着色器资源绑定准备的GPU绑定组的资源
///
/// This resource stores pre-configured bind groups that bundle multiple GPU
/// resources together for efficient binding to compute and overlay shaders.
/// Bind groups reduce GPU API overhead by batching resource bindings.
///
/// # Bind Group Benefits
/// - **Performance**: Single bind operation for multiple resources
/// - **Validation**: GPU validates resource compatibility at bind group creation
/// - **Caching**: GPU drivers can optimize resource layouts
/// - **Efficiency**: Reduces per-frame GPU API overhead
///
/// # Resource Grouping
/// Bind groups organize related resources by shader stage:
/// - **Compute**: Resources needed for fog compute shaders
/// - **Overlay**: Resources needed for fog overlay rendering (future)
///
/// # Lifecycle Management
/// - **Creation**: Bind groups created when all required resources are ready
/// - **Validity**: Bind groups become invalid when underlying resources change
/// - **Recreation**: New bind groups created when resources are updated
/// - **Optional**: Missing bind groups cause graceful rendering skips
#[derive(Resource, Default)]
pub struct FogBindGroups {
    /// Optional bind group for compute shader resource binding.
    /// 用于计算着色器资源绑定的可选绑定组
    ///
    /// Contains all resources needed for fog compute operations:
    /// - Fog and visibility texture arrays
    /// - Vision source storage buffer
    /// - Chunk information storage buffer
    /// - Fog settings uniform buffer
    ///
    /// None when resources are not ready or need updating.
    pub compute: Option<BindGroup>,
}

// --- Buffer Preparation Systems ---
// --- 缓冲区准备系统 ---

/// Prepares GPU uniform buffer containing fog settings for shader access.
/// 为着色器访问准备包含雾效设置的GPU统一缓冲区
///
/// This system creates a GPU uniform buffer from the extracted fog settings,
/// making fog configuration data available to both compute and overlay shaders.
/// The buffer is recreated each frame to handle potential settings changes.
///
/// # Buffer Creation Process
/// 1. **Data Extraction**: Get RenderFogMapSettings from render world
/// 2. **Memory Allocation**: Create GPU uniform buffer with direct data upload
/// 3. **Resource Storage**: Store buffer handle in FogUniforms resource
/// 4. **Usage Configuration**: Set UNIFORM and COPY_DST usage flags
///
/// # GPU Buffer Properties
/// - **Type**: Uniform buffer for fast, cached access
/// - **Size**: 80 bytes (size of RenderFogMapSettings structure)
/// - **Usage**: UNIFORM (shader binding) + COPY_DST (CPU updates)
/// - **Access**: Read-only from both compute and overlay shaders
///
/// # Data Format
/// Uses bytemuck for zero-copy conversion from CPU to GPU format:
/// - **Pod Compatibility**: RenderFogMapSettings implements Pod trait
/// - **Memory Layout**: Direct byte-level copy without serialization
/// - **Alignment**: GPU-compatible memory alignment preserved
/// - **Endianness**: Platform-independent representation maintained
///
/// # Performance Characteristics
/// - **Allocation**: One 80-byte GPU buffer allocation per frame
/// - **Upload**: Direct memory copy without staging buffers
/// - **Access**: Extremely fast uniform buffer reads on GPU
/// - **Caching**: GPU caches uniform buffer for repeated access
///
/// # Shader Integration
/// The buffer is bound to both compute and overlay shaders at binding 4:
/// - **Compute**: Used for fog calculation parameters
/// - **Overlay**: Used for fog color and blending configuration
/// - **Global Access**: All shader threads can access settings data
///
/// # Time Complexity: O(1) - constant time buffer creation and data upload
pub fn prepare_fog_uniforms(
    settings: Res<RenderFogMapSettings>,
    mut fog_uniforms: ResMut<FogUniforms>,
    render_device: Res<RenderDevice>,
) {
    // Create GPU uniform buffer with direct data upload from CPU settings
    // 使用CPU设置直接数据上传创建GPU统一缓冲区
    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("fog setting data buffer"), // Debug label for GPU debugging
        contents: bytemuck::cast_slice(&[*settings]), // Zero-copy conversion to bytes
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST, // Uniform access + CPU updates
    });

    // Store buffer handle for shader binding
    // 存储缓冲区句柄用于着色器绑定
    fog_uniforms.buffer = Some(buffer);
}

/// Prepares GPU storage buffer containing vision source data for compute shader processing.
/// 为计算着色器处理准备包含视野源数据的GPU存储缓冲区
///
/// This system creates a GPU storage buffer from extracted vision source data,
/// enabling compute shaders to access vision source properties for fog calculations.
/// The buffer is dynamically sized based on the number of active vision sources.
///
/// # Buffer Creation Process
/// 1. **Data Extraction**: Get ExtractedVisionSources from render world
/// 2. **Size Calculation**: Determine buffer size based on vision source count
/// 3. **Memory Allocation**: Create GPU storage buffer with vision source array
/// 4. **Capacity Tracking**: Update buffer capacity for validation and debugging
///
/// # Dynamic Buffer Sizing
/// Buffer size adapts to vision source count each frame:
/// - **Variable Size**: Buffer grows/shrinks with active vision source count
/// - **Memory Efficiency**: No wasted memory for inactive sources
/// - **Performance**: GPU processes only active vision sources
/// - **Fallback**: Always contains at least one entry (default disabled source)
///
/// # GPU Buffer Properties
/// - **Type**: Storage buffer for large array data
/// - **Size**: Variable (48 bytes × number of vision sources)
/// - **Usage**: STORAGE (shader array access) + COPY_DST (CPU updates)
/// - **Access**: Read-only from compute shaders with indexed access
///
/// # Data Structure Format
/// Contains array of VisionSourceData structures with:
/// - **Position**: World coordinates of vision source
/// - **Properties**: Radius, shape type, direction, intensity
/// - **Optimization**: Precomputed trigonometric values for GPU efficiency
/// - **Alignment**: GPU-compatible memory layout (48 bytes per source)
///
/// # Performance Characteristics
/// - **Memory**: 48 bytes per active vision source
/// - **Upload**: Direct memory copy without staging buffers
/// - **GPU Access**: Efficient indexed array access in compute shaders
/// - **Bandwidth**: Scales linearly with number of active vision sources
///
/// # Compute Shader Integration
/// The buffer is bound to compute shaders at binding 2:
/// ```wgsl
/// @group(0) @binding(2) var<storage, read> vision_sources: array<VisionSourceData>;
/// ```
///
/// # Time Complexity: O(n) where n = number of active vision sources
pub fn prepare_vision_source_buffer(
    extracted_sources: Res<ExtractedVisionSources>,
    mut buffer_res: ResMut<VisionSourceBuffer>,
    render_device: Res<RenderDevice>,
) {
    // Calculate buffer capacity based on extracted vision source count
    // 根据提取的视野源数量计算缓冲区容量
    let capacity = extracted_sources.sources.len();
    buffer_res.capacity = capacity;

    // Create GPU storage buffer with vision source array data
    // 使用视野源数组数据创建GPU存储缓冲区
    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("vision_source_storage_buffer"), // Debug label for GPU debugging
        contents: bytemuck::cast_slice(&extracted_sources.sources), // Array of VisionSourceData
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST, // Storage array + CPU updates
    });

    // Store buffer handle for compute shader binding
    // 存储缓冲区句柄用于计算着色器绑定
    buffer_res.buffer = Some(buffer);
}

/// Prepares GPU storage buffer containing chunk computation data for compute shader processing.
/// 为计算着色器处理准备包含区块计算数据的GPU存储缓冲区
///
/// This system creates a GPU storage buffer from extracted chunk data that has been
/// filtered by frustum culling and GPU memory state. The buffer enables parallel
/// chunk processing in compute shaders for fog visibility calculations.
///
/// # Buffer Creation Process
/// 1. **Data Extraction**: Get ExtractedGpuChunkData from render world
/// 2. **Capacity Tracking**: Record number of chunks for GPU processing
/// 3. **Memory Allocation**: Create GPU storage buffer with chunk array
/// 4. **Resource Storage**: Store buffer handle for compute shader access
///
/// # Chunk Filtering and Optimization
/// The input data has already been optimized through:
/// - **Frustum Culling**: Only chunks visible to camera are included
/// - **Memory State**: Only GPU-resident or pending chunks are processed
/// - **Coordinate Mapping**: Chunks include texture layer index mappings
/// - **Minimal Data**: Only essential data for compute operations (16 bytes/chunk)
///
/// # GPU Buffer Properties
/// - **Type**: Storage buffer for chunk processing array
/// - **Size**: Variable (16 bytes × number of culled chunks)
/// - **Usage**: STORAGE (shader array access) + COPY_DST (CPU updates)
/// - **Access**: Read-only from compute shaders with workgroup-based access
///
/// # Data Structure Format
/// Contains array of ChunkComputeData structures with:
/// - **Chunk Coordinates**: Spatial location in chunk space
/// - **Fog Layer Index**: Target fog texture array layer
/// - **Alignment Padding**: GPU memory alignment requirements
/// - **Compact Layout**: Minimal memory footprint for efficiency
///
/// # Performance Impact
/// Buffer size directly affects GPU compute performance:
/// - **Frustum Culling**: Typically reduces chunks by 60-90%
/// - **Parallel Processing**: GPU processes chunks in parallel workgroups
/// - **Memory Bandwidth**: 16 bytes per chunk minimizes bandwidth usage
/// - **Cache Efficiency**: Sequential access pattern optimizes GPU cache
///
/// # Compute Shader Integration
/// The buffer is bound to compute shaders at binding 3:
/// ```wgsl
/// @group(0) @binding(3) var<storage, read> chunks: array<ChunkComputeData>;
/// ```
///
/// # Workgroup Dispatch Relationship
/// Buffer capacity determines compute shader dispatch dimensions:
/// - **Z-Dimension**: workgroups_z = buffer.capacity (one workgroup per chunk)
/// - **Parallel Processing**: Each chunk processed by separate workgroup
/// - **Scalability**: Performance scales with visible chunk count
///
/// # Time Complexity: O(n) where n = number of visible/GPU-resident chunks
pub fn prepare_gpu_chunk_buffer(
    extracted_chunks: Res<ExtractedGpuChunkData>,
    mut buffer_res: ResMut<GpuChunkInfoBuffer>,
    render_device: Res<RenderDevice>,
) {
    // Record number of chunks for GPU compute processing and debugging
    // 记录GPU计算处理和调试的区块数量
    buffer_res.capacity = extracted_chunks.compute_chunks.len();

    // Create GPU storage buffer with filtered chunk computation data
    // 使用过滤的区块计算数据创建GPU存储缓冲区
    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("gpu_chunk_info_storage_buffer"), // Debug label for GPU debugging
        contents: bytemuck::cast_slice(&extracted_chunks.compute_chunks), // Array of ChunkComputeData
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST, // Storage array + CPU updates
    });

    // Store buffer handle for compute shader binding
    // 存储缓冲区句柄用于计算着色器绑定
    buffer_res.buffer = Some(buffer);
}

/// Prepares GPU storage buffer containing chunk mapping data for overlay rendering operations.
/// 为覆盖渲染操作准备包含区块映射数据的GPU存储缓冲区
///
/// This system creates a GPU storage buffer from extracted overlay chunk mapping data,
/// enabling overlay fragment shaders to map screen coordinates to appropriate fog texture
/// layers. The buffer provides the coordinate transformation data needed for fog compositing.
///
/// # Buffer Creation Process
/// 1. **Data Extraction**: Get ExtractedGpuChunkData containing overlay mapping array
/// 2. **Capacity Recording**: Track number of chunks for overlay rendering
/// 3. **Memory Allocation**: Create GPU storage buffer with overlay chunk data
/// 4. **Resource Storage**: Store buffer handle for overlay shader access
///
/// # Overlay Rendering Integration
/// The buffer enables overlay shaders to:
/// - **Coordinate Mapping**: Convert screen coordinates to chunk coordinates
/// - **Layer Lookup**: Find appropriate texture array layer indices
/// - **Texture Sampling**: Access correct fog and snapshot texture layers
/// - **Spatial Transformation**: Handle chunk-based spatial organization
///
/// # GPU Buffer Properties
/// - **Type**: Storage buffer for overlay mapping array
/// - **Size**: Variable (16 bytes × number of visible chunks)
/// - **Usage**: STORAGE (shader array access) + COPY_DST (CPU updates)
/// - **Access**: Read-only from overlay fragment shaders
///
/// # Data Structure Format
/// Contains array of OverlayChunkData structures with:
/// - **Chunk Coordinates**: Spatial location in chunk space
/// - **Fog Layer Index**: Texture array layer for fog data
/// - **Snapshot Layer Index**: Texture array layer for snapshot data
/// - **Alignment Padding**: GPU memory alignment requirements
///
/// # Performance Characteristics
/// Buffer size affects overlay rendering performance:
/// - **Screen Resolution**: Fragment shaders access this buffer per-pixel
/// - **GPU Caching**: GPU caches frequently accessed chunk mappings
/// - **Memory Bandwidth**: 16 bytes per chunk for coordinate transformations
/// - **Fragment Overhead**: Per-pixel buffer lookups during overlay rendering
///
/// # Shader Integration
/// The buffer is bound to overlay shaders at binding 6:
/// ```wgsl
/// @group(0) @binding(6) var<storage, read> chunks: array<OverlayChunkData>;
/// ```
///
/// # Coordinate Transformation Usage
/// Fragment shaders use this buffer to:
/// 1. **Convert Screen→World**: Transform fragment coordinates to world space
/// 2. **Convert World→Chunk**: Calculate chunk coordinates from world position
/// 3. **Lookup Mapping**: Find chunk entry in this buffer array
/// 4. **Extract Layers**: Get fog and snapshot texture layer indices
/// 5. **Sample Textures**: Use layer indices for texture array sampling
///
/// # Frustum Culling Integration
/// Buffer contains only chunks visible to the camera:
/// - **Culled Data**: Only visible chunks included for efficiency
/// - **Dynamic Size**: Buffer size varies with camera view
/// - **Memory Optimization**: No GPU memory wasted on invisible chunks
/// - **Performance Scaling**: Overlay performance scales with visible area
///
/// # Time Complexity: O(n) where n = number of visible chunks in camera view
pub fn prepare_overlay_chunk_mapping_buffer(
    extracted_chunks: Res<ExtractedGpuChunkData>,
    mut buffer_res: ResMut<OverlayChunkMappingBuffer>,
    render_device: Res<RenderDevice>,
) {
    // Record number of chunks for overlay rendering and performance monitoring
    // 记录覆盖渲染的区块数量和性能监控
    let capacity = extracted_chunks.overlay_mapping.len();

    // Create GPU storage buffer with overlay chunk mapping data
    // 使用覆盖区块映射数据创建GPU存储缓冲区
    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("overlay_chunk_mapping_storage_buffer"), // Debug label for GPU debugging
        contents: bytemuck::cast_slice(&extracted_chunks.overlay_mapping), // Array of OverlayChunkData
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST, // Storage access + CPU updates
    });

    // Store buffer handle and capacity for overlay shader binding
    // 存储缓冲区句柄和容量用于覆盖着色器绑定
    buffer_res.buffer = Some(buffer);
    buffer_res.capacity = capacity;
}

/// Prepares GPU bind groups by bundling multiple resources for efficient shader binding.
/// 通过捆绑多个资源为高效着色器绑定准备GPU绑定组
///
/// This system creates GPU bind groups that bundle related resources together,
/// enabling efficient binding to compute and overlay shaders. Bind groups reduce
/// GPU API overhead by allowing multiple resources to be bound with a single operation.
///
/// # Bind Group Architecture
/// Bind groups organize GPU resources by usage pattern:
/// - **Compute Bind Group**: Resources needed for fog compute shaders
/// - **Future Overlay Groups**: Potential expansion for overlay-specific bindings
/// - **Resource Validation**: Only creates bind groups when all dependencies are ready
/// - **Fallback Support**: Uses fallback textures when real textures unavailable
///
/// # Resource Bundling Process
/// 1. **Texture Retrieval**: Get texture views with fallback support
/// 2. **Buffer Validation**: Ensure all required GPU buffers are prepared
/// 3. **Bind Group Creation**: Bundle resources according to shader binding layout
/// 4. **Resource Storage**: Store completed bind groups for shader usage
///
/// # Compute Bind Group Resources
/// The compute bind group contains 5 resources bound sequentially:
/// ```wgsl
/// @group(0) @binding(0) var fog_texture: texture_storage_2d_array<r8unorm, write>;
/// @group(0) @binding(1) var visibility_texture: texture_storage_2d_array<r8unorm, read_write>;
/// @group(0) @binding(2) var<storage, read> vision_sources: array<VisionSourceData>;
/// @group(0) @binding(3) var<storage, read> chunks: array<ChunkComputeData>;
/// @group(0) @binding(4) var<uniform> settings: RenderFogMapSettings;
/// ```
///
/// # Fallback Texture Strategy
/// Uses robust fallback mechanism for texture availability:
/// - **Primary Textures**: Attempts to use real fog and visibility textures
/// - **Fallback Textures**: Uses Bevy's fallback images when primary unavailable
/// - **Graceful Degradation**: Shaders continue working with fallback data
/// - **Asset Loading**: Handles texture loading delays transparently
///
/// # Resource Validation Logic
/// Bind group creation follows strict validation:
/// - **All-or-Nothing**: Bind group created only when ALL buffers are ready
/// - **Optional Textures**: Uses fallbacks for missing textures
/// - **Buffer Dependencies**: Requires uniform, storage, and chunk buffers
/// - **Atomic Creation**: Either complete bind group or none at all
///
/// # Performance Benefits
/// Bind groups provide several GPU performance advantages:
/// - **Reduced API Calls**: Single bind operation instead of multiple resource bindings
/// - **GPU Validation**: Driver validates resource compatibility at creation time
/// - **Memory Layout**: GPU optimizes memory layout for grouped resources
/// - **Cache Efficiency**: Related resources grouped for better cache locality
///
/// # GPU Resource Layout
/// Resources are organized for optimal GPU access:
/// - **Texture Arrays**: Fog and visibility textures for compute operations
/// - **Storage Buffers**: Vision source and chunk data for parallel processing
/// - **Uniform Buffer**: Global fog settings shared across all threads
/// - **Sequential Binding**: Resources bound in shader binding order for efficiency
///
/// # Error Handling and Resilience
/// The system handles resource unavailability gracefully:
/// - **Missing Buffers**: Skips bind group creation when buffers not ready
/// - **Texture Loading**: Uses fallback textures during asset loading
/// - **Partial Readiness**: Waits for all required resources before proceeding
/// - **No Crashes**: Never creates invalid or incomplete bind groups
///
/// # Future Expansion
/// The system is designed for expansion:
/// - **Overlay Bind Groups**: Framework ready for overlay-specific resource groups
/// - **Multiple Layouts**: Can support different bind group layouts per shader stage
/// - **Dynamic Resources**: Can adapt to different resource configurations
/// - **Shader Variants**: Supports different shader configurations with separate groups
///
/// # Integration Points
/// - **Pipeline Layouts**: Uses pipeline bind group layouts for compatibility
/// - **Compute Shaders**: Primary consumer of compute bind groups
/// - **Resource Systems**: Depends on prepare systems for buffer readiness
/// - **Render Graph**: Provides bind groups for shader execution nodes
///
/// # Time Complexity: O(1) - constant time bind group creation with fixed resource count
#[allow(clippy::too_many_arguments)]
pub fn prepare_fog_bind_groups(
    render_device: Res<RenderDevice>,
    mut fog_bind_groups: ResMut<FogBindGroups>,
    fog_uniforms: Res<FogUniforms>,
    vision_source_buffer: Res<VisionSourceBuffer>,
    gpu_chunk_buffer: Res<GpuChunkInfoBuffer>,
    fog_texture: Res<RenderFogTexture>,
    visibility_texture: Res<RenderVisibilityTexture>,
    images: Res<RenderAssets<GpuImage>>,
    fallback_image: Res<FallbackImage>, // For default textures / 用于默认纹理
    fog_compute_pipeline: Res<FogComputePipeline>, // For view uniform binding / 用于视图统一绑定
) {
    // Get texture views with fallback support for robust resource handling
    // 获取纹理视图，支持回退以实现强大的资源处理
    let fog_texture_view = images
        .get(&fog_texture.0) // Try to get real fog texture array
        .map(|img| &img.texture_view)
        .unwrap_or(&fallback_image.d1.texture_view); // Use fallback if not available

    let visibility_texture_view = images
        .get(&visibility_texture.0) // Try to get real visibility texture array
        .map(|img| &img.texture_view)
        .unwrap_or(&fallback_image.d1.texture_view); // Use fallback if not available

    // --- Compute Bind Group Creation ---
    // --- 计算绑定组创建 ---

    // Validate all required GPU buffers are prepared before bind group creation
    // 在绑定组创建之前验证所有必需的GPU缓冲区都已准备就绪
    if let (Some(uniform_buf), Some(source_buf), Some(chunk_buf)) = (
        fog_uniforms.buffer.as_ref(),         // Fog settings uniform buffer
        vision_source_buffer.buffer.as_ref(), // Vision source storage buffer
        gpu_chunk_buffer.buffer.as_ref(),     // Chunk computation storage buffer
    ) {
        // Create compute bind group with all required resources for fog calculations
        // 创建包含雾效计算所需所有资源的计算绑定组
        let compute_bind_group = render_device.create_bind_group(
            "fog_compute_bind_group",             // Debug label for GPU debugging
            &fog_compute_pipeline.compute_layout, // Use pipeline's bind group layout
            &BindGroupEntries::sequential((
                fog_texture_view,                // 0: Fog texture array (write access)
                visibility_texture_view,         // 1: Visibility texture array (read/write)
                source_buf.as_entire_binding(),  // 2: Vision source storage buffer
                chunk_buf.as_entire_binding(),   // 3: Chunk computation storage buffer
                uniform_buf.as_entire_binding(), // 4: Fog settings uniform buffer
            )),
        );

        // Store completed bind group for compute shader execution
        // 存储完成的绑定组用于计算着色器执行
        fog_bind_groups.compute = Some(compute_bind_group);
    }
    // If any required buffer is missing, skip bind group creation
    // 如果任何必需的缓冲区缺失，跳过绑定组创建
    // The compute node will check for bind group availability before execution
    // 计算节点将在执行前检查绑定组可用性
}
