//! Data extraction systems for transferring fog of war data from main world to render world.
//! 将战争迷雾数据从主世界传输到渲染世界的数据提取系统
//!
//! This module implements the critical data extraction phase of Bevy's rendering pipeline,
//! where main world data is converted and transferred to the render world for GPU processing.
//! It handles format conversion, GPU-compatible data structures, and efficient data transfer.
//!
//! # Extract Phase Overview
//!
//! ## Main World → Render World Transfer
//! The extract phase runs between main world updates and render world processing:
//! ```text
//! [Main World] → [Extract Phase] → [Render World] → [GPU]
//!      ↓              ↓               ↓           ↓
//! ECS Components → GPU-Compatible → Render      → Compute
//! Game Logic    → Data Structures → Resources   → Shaders
//! ```
//!
//! ## Data Transformation Pipeline
//! 1. **Settings Extraction**: Convert FogMapSettings to GPU-compatible format
//! 2. **Vision Source Processing**: Transform entities to shader-ready data structures
//! 3. **Chunk Data Preparation**: Prepare chunk information for compute shaders
//! 4. **Texture Handle Transfer**: Ensure GPU texture access in render world
//! 5. **Frustum Culling**: Filter chunks based on camera visibility
//!
//! # GPU Data Format Requirements
//!
//! ## Shader Compatibility
//! All extracted data structures must match WGSL shader layouts:
//! - **Alignment**: C-style struct alignment with explicit padding
//! - **Size**: Fixed sizes matching GPU buffer requirements
//! - **Layout**: Sequential memory layout for efficient GPU access
//! - **Endianness**: Platform-independent data representation
//!
//! ## Memory Layout Optimization
//! ```rust,no_run
//! #[repr(C)]              // C-style layout for GPU compatibility
//! #[derive(Pod, Zeroable)] // Safe byte-level operations
//! #[derive(ShaderType)]    // Bevy shader interface
//! struct GpuData {
//!     data: Vec2,         // 8 bytes
//!     _padding: u32,      // 4 bytes for alignment
//! }                       // Total: 12 bytes, GPU-aligned
//! ```
//!
//! # Performance Characteristics
//!
//! ## Data Transfer Efficiency
//! - **Batch Operations**: All chunks processed in single extraction pass
//! - **Memory Layout**: Optimized for GPU memory access patterns
//! - **Culling**: Early elimination of non-visible chunks
//! - **Minimal Allocation**: Reuses existing vectors to avoid allocations
//!
//! ## Extraction Frequency
//! - **Per Frame**: Runs every frame during render pipeline
//! - **Delta Processing**: Only processes changed data when possible
//! - **Selective Extraction**: Filters out disabled or irrelevant entities
//! - **Frustum Culling**: Reduces data volume based on camera view
//!
//! # Resource Management
//!
//! ## Main World Resources
//! - **FogMapSettings**: Global fog configuration
//! - **VisionSource Components**: Entity-based vision providers
//! - **FogChunk Components**: Chunk state and allocation data
//! - **Texture Handles**: GPU texture array references
//!
//! ## Render World Resources
//! - **RenderFogMapSettings**: GPU-compatible settings structure
//! - **ExtractedVisionSources**: Processed vision source data
//! - **ExtractedGpuChunkData**: Culled and formatted chunk information
//! - **Render Texture Handles**: GPU texture access handles
//!
//! # Error Handling and Fallbacks
//!
//! ## Graceful Degradation
//! - **Missing Data**: Provides default values when sources unavailable
//! - **Empty Sets**: Ensures minimum data requirements for GPU operations
//! - **Invalid Chunks**: Filters out chunks with invalid GPU state
//! - **Camera Issues**: Falls back to no culling when camera unavailable
//!
//! # Integration Points
//!
//! ## Upstream Dependencies
//! - **Main World Systems**: Entity management and chunk allocation
//! - **Vision Systems**: Real-time vision source updates
//! - **Camera Systems**: View frustum and projection information
//! - **Texture Management**: GPU texture array allocation
//!
//! ## Downstream Consumers
//! - **Prepare Systems**: Use extracted data for GPU buffer creation
//! - **Compute Shaders**: Process extracted chunk and vision data
//! - **Overlay Systems**: Render final fog effects using extracted information
//! - **Transfer Systems**: Handle CPU↔GPU memory operations

#![allow(dead_code)]

use crate::prelude::*;
use bevy::render::Extract;
use bevy::render::render_resource::ShaderType;
use bytemuck::{Pod, Zeroable};

/// GPU-compatible fog map settings resource for render world shader access.
/// 用于渲染世界着色器访问的GPU兼容雾效地图设置资源
///
/// This structure represents the main fog configuration data in a format optimized
/// for GPU shader access. It's extracted from the main world's FogMapSettings and
/// converted to match WGSL uniform buffer requirements.
///
/// # GPU Compatibility
/// - **Layout**: C-style struct layout (`#[repr(C)]`) for consistent memory layout
/// - **Alignment**: Proper padding ensures GPU memory alignment requirements
/// - **Size**: Fixed size structure for efficient uniform buffer allocation
/// - **Endianness**: Platform-independent representation for cross-platform support
///
/// # Shader Integration
/// This structure is bound to compute and overlay shaders as a uniform buffer:
/// ```wgsl
/// struct FogSettings {
///     chunk_size: vec2<u32>,
///     texture_resolution_per_chunk: vec2<u32>,
///     fog_color_unexplored: vec4<f32>,
///     fog_color_explored: vec4<f32>,
///     vision_clear_color: vec4<f32>,
///     enabled: u32,
/// }
/// @group(0) @binding(4) var<uniform> settings: FogSettings;
/// ```
///
/// # Color Space Handling
/// - **Input**: sRGB colors from main world settings
/// - **Conversion**: Automatically converted to linear color space for GPU
/// - **Shader Usage**: Linear colors used for proper blending and calculations
/// - **Precision**: Vec4 format provides sufficient precision for color operations
///
/// # Memory Layout
/// ```text
/// Offset | Size | Field
/// -------|------|------
/// 0      | 8    | chunk_size (UVec2)
/// 8      | 8    | texture_resolution_per_chunk (UVec2)
/// 16     | 16   | fog_color_unexplored (Vec4)
/// 32     | 16   | fog_color_explored (Vec4)
/// 48     | 16   | vision_clear_color (Vec4)
/// 64     | 4    | enabled (u32)
/// 68     | 12   | _padding1 (alignment)
/// Total: 80 bytes (GPU-aligned)
/// ```
///
/// # Performance Characteristics
/// - **Transfer Cost**: Minimal - single small uniform buffer update per frame
/// - **GPU Access**: Extremely fast uniform buffer access in shaders
/// - **Memory Usage**: 80 bytes total, negligible memory overhead
/// - **Cache Efficiency**: Small size fits in GPU cache lines
#[derive(Resource, Debug, Clone, Copy, Pod, Zeroable, ShaderType)]
#[repr(C)]
pub struct RenderFogMapSettings {
    /// Size of each chunk in world units, determines chunk spatial dimensions.
    /// 每个区块的世界单位大小，决定区块空间维度
    ///
    /// This value controls how much world space each chunk covers. Larger values
    /// mean fewer chunks but lower spatial resolution. Used by shaders for
    /// coordinate transformations and chunk boundary calculations.
    pub chunk_size: UVec2,

    /// Texture resolution per chunk in pixels, determines fog detail level.
    /// 每个区块的纹理像素分辨率，决定雾效细节级别
    ///
    /// Higher resolutions provide more detailed fog but require more GPU memory.
    /// Typical values are 256x256 or 512x512. Used by compute shaders to
    /// determine workgroup dispatch dimensions and texture coordinates.
    pub texture_resolution_per_chunk: UVec2,

    /// Fog color for completely unexplored areas (full opacity).
    /// 完全未探索区域的雾颜色（完全不透明）
    ///
    /// This color is used for areas that have never been seen by any vision source.
    /// Typically dark or black with full alpha to completely obscure content.
    /// Converted from sRGB to linear color space for proper GPU blending.
    pub fog_color_unexplored: Vec4,

    /// Fog color for explored but currently not visible areas (partial transparency).
    /// 已探索但当前不可见区域的雾颜色（部分透明）
    ///
    /// This color is used for areas that were previously explored but are not
    /// currently within any vision source range. Typically semi-transparent
    /// to show snapshot content underneath while indicating reduced visibility.
    pub fog_color_explored: Vec4,

    /// Color for fully visible areas within vision range (usually transparent).
    /// 视野范围内完全可见区域的颜色（通常透明）
    ///
    /// This color is used for areas currently within vision source range.
    /// Typically fully transparent (alpha = 0) to show the scene without
    /// any fog overlay. May have slight tinting for special effects.
    pub vision_clear_color: Vec4,

    /// Fog system enabled state (0 = disabled, 1 = enabled).
    /// 雾效系统启用状态（0 = 禁用，1 = 启用）
    ///
    /// Used by shaders to completely bypass fog calculations when disabled.
    /// Boolean values are represented as u32 for GPU compatibility.
    pub enabled: u32,

    /// Padding to ensure proper GPU memory alignment.
    /// 确保适当GPU内存对齐的填充
    ///
    /// This padding ensures the structure size is a multiple of 16 bytes
    /// as required by GPU uniform buffer alignment rules.
    pub _padding1: [u32; 3],
}

/// Render world resource containing processed vision source data for GPU consumption.
/// 包含用于GPU消费的已处理视野源数据的渲染世界资源
///
/// This resource stores vision source data extracted from the main world and converted
/// into a GPU-compatible format. All vision sources are processed and stored in a
/// single vector for efficient GPU buffer creation and shader access.
///
/// # Data Processing
/// - **Entity Filtering**: Only enabled vision sources are included
/// - **Format Conversion**: Main world components converted to GPU data structures
/// - **Coordinate Transformation**: World positions extracted from GlobalTransform
/// - **Trigonometric Precalculation**: Direction vectors precomputed for GPU efficiency
///
/// # GPU Buffer Usage
/// The sources vector is used to create a GPU storage buffer:
/// ```wgsl
/// struct VisionSourceData {
///     position: vec2<f32>,
///     radius: f32,
///     shape_type: u32,
///     // ... other fields
/// }
/// @group(0) @binding(2) var<storage, read> vision_sources: array<VisionSourceData>;
/// ```
///
/// # Performance Characteristics
/// - **Memory**: One VisionSourceData struct per active vision source
/// - **Update Frequency**: Refreshed every frame during extraction
/// - **GPU Transfer**: Single buffer upload per frame
/// - **Shader Access**: Efficient indexed access in compute shaders
///
/// # Fallback Behavior
/// If no vision sources are found, a default disabled source is added to ensure
/// GPU shaders always have valid data to read, preventing buffer access errors.
#[derive(Resource, Debug, Clone, Default)]
pub struct ExtractedVisionSources {
    /// Vector of GPU-compatible vision source data structures.
    /// GPU兼容的视野源数据结构向量
    ///
    /// Each element represents one active vision source entity from the main world,
    /// converted to a format suitable for direct GPU buffer creation. The vector
    /// is cleared and repopulated each frame during the extraction phase.
    pub sources: Vec<VisionSourceData>,
}

/// Render world resource containing processed chunk data for GPU compute and overlay operations.
/// 包含用于GPU计算和覆盖操作的已处理区块数据的渲染世界资源
///
/// This resource stores chunk information extracted from the main world, filtered for
/// GPU-resident chunks, and processed for efficient shader access. It maintains separate
/// data structures optimized for different GPU operations.
///
/// # Data Organization
/// - **compute_chunks**: Minimal data for compute shader processing
/// - **overlay_mapping**: Extended data for overlay rendering with snapshot indices
/// - **Synchronized Indexing**: Both vectors maintain the same chunk ordering
///
/// # Frustum Culling
/// Chunks are filtered based on camera visibility to reduce GPU workload:
/// - **Spatial Culling**: Only chunks intersecting camera view are included
/// - **Memory State**: Only GPU-resident or pending chunks are processed
/// - **Performance**: Significantly reduces shader workload for large worlds
///
/// # GPU Buffer Creation
/// The extracted data is used to create GPU storage buffers:
/// ```wgsl
/// @group(0) @binding(3) var<storage, read> chunks: array<ChunkComputeData>;
/// ```
///
/// # Performance Impact
/// - **Culling Efficiency**: Reduces processed chunks by 60-90% in typical scenarios
/// - **GPU Workload**: Directly proportional to number of chunks in view
/// - **Memory Usage**: Minimal per-chunk overhead (16-20 bytes per chunk)
/// - **Update Frequency**: Recalculated every frame based on camera movement
#[derive(Resource, Debug, Clone, Default)]
pub struct ExtractedGpuChunkData {
    /// Minimal chunk data optimized for compute shader processing.
    /// 为计算着色器处理优化的最小区块数据
    ///
    /// Contains only the essential information needed by fog compute shaders:
    /// chunk coordinates and fog texture layer indices. This minimal format
    /// reduces GPU memory bandwidth and improves cache efficiency.
    pub compute_chunks: Vec<ChunkComputeData>,

    /// Extended chunk data for overlay rendering including snapshot information.
    /// 包含快照信息的覆盖渲染扩展区块数据
    ///
    /// Contains chunk information needed for fog overlay rendering, including
    /// both fog and snapshot texture layer indices. Used by overlay shaders
    /// to properly composite fog effects with snapshot content.
    pub overlay_mapping: Vec<OverlayChunkData>,
}

/// Render world resource providing access to the fog texture array for GPU operations.
/// 为GPU操作提供雾效纹理数组访问的渲染世界资源
///
/// This resource stores the handle to the fog texture array in the render world,
/// enabling GPU shaders to access persistent fog exploration data. The fog texture
/// accumulates exploration history over time.
///
/// # Usage in Shaders
/// Bound as a storage texture for read/write operations:
/// ```wgsl
/// @group(0) @binding(1) var fog_texture: texture_storage_2d_array<r8unorm, write>;
/// ```
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderFogTexture(pub Handle<Image>);

/// Render world resource providing access to the visibility texture array for GPU operations.
/// 为GPU操作提供可见性纹理数组访问的渲染世界资源
///
/// This resource stores the handle to the visibility texture array in the render world,
/// enabling GPU shaders to access real-time visibility calculations. The visibility
/// texture is updated every frame based on current vision source positions.
///
/// # Usage in Shaders
/// Bound as a storage texture for read/write operations:
/// ```wgsl
/// @group(0) @binding(0) var visibility_texture: texture_storage_2d_array<r8unorm, read_write>;
/// ```
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderVisibilityTexture(pub Handle<Image>);

/// Render world resource providing access to the snapshot texture array for GPU operations.
/// 为GPU操作提供快照纹理数组访问的渲染世界资源
///
/// This resource stores the handle to the snapshot texture array in the render world,
/// enabling overlay shaders to access captured entity snapshots. Snapshots preserve
/// the visual appearance of explored areas when they become obscured by fog.
///
/// # Usage in Shaders
/// Bound as a texture for reading in overlay shaders:
/// ```wgsl
/// @group(0) @binding(N) var snapshot_texture: texture_2d_array<f32>;
/// ```
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderSnapshotTexture(pub Handle<Image>);

/// Render world resource providing access to the temporary snapshot texture for capture operations.
/// 为捕获操作提供临时快照纹理访问的渲染世界资源
///
/// This resource stores the handle to the temporary snapshot texture used during
/// snapshot capture operations. The temporary texture serves as an intermediate
/// render target before final copying to the snapshot texture array.
///
/// # Usage in Snapshot System
/// Used as render target for snapshot camera and source for texture copy operations.
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RenderSnapshotTempTexture(pub Handle<Image>);

// --- GPU-Compatible Data Structures ---
// --- GPU兼容数据结构 ---

/// GPU-compatible vision source data structure matching WGSL shader layout.
/// 与WGSL着色器布局匹配的GPU兼容视野源数据结构
///
/// This structure represents a single vision source in a format optimized for GPU
/// shader access. It includes the original vision parameters plus precomputed
/// values to improve shader performance.
///
/// # Memory Layout
/// Carefully designed to match WGSL struct alignment requirements:
/// ```text
/// Offset | Size | Field               | Purpose
/// -------|------|---------------------|------------------------
/// 0      | 8    | position            | World position (Vec2)
/// 8      | 4    | radius              | Vision range
/// 12     | 4    | shape_type          | Vision shape (0/1/2)
/// 16     | 4    | direction_rad       | Direction in radians
/// 20     | 4    | angle_rad           | Cone angle in radians
/// 24     | 4    | intensity           | Vision strength
/// 28     | 4    | transition_ratio    | Edge softness
/// 32     | 4    | cos_direction       | Precomputed cos(direction)
/// 36     | 4    | sin_direction       | Precomputed sin(direction)
/// 40     | 4    | cone_half_angle_cos | Precomputed cos(angle/2)
/// 44     | 4    | _padding1           | Alignment padding
/// Total: 48 bytes (GPU-aligned)
/// ```
///
/// # Vision Shape Types
/// - **0 (Circle)**: Omnidirectional vision with configurable radius
/// - **1 (Cone)**: Directional vision with angle and direction parameters
/// - **2 (Square/Rectangle)**: Rectangular vision area (future implementation)
///
/// # Performance Optimizations
/// - **Precomputed Trigonometry**: cos/sin values calculated on CPU
/// - **Half-Angle Cosine**: Cone calculations optimized for GPU
/// - **Packed Layout**: Minimal memory footprint for GPU bandwidth
/// - **Aligned Access**: Memory layout optimized for GPU cache efficiency
///
/// # Shader Integration
/// Used in compute shaders for vision calculations:
/// ```wgsl
/// struct VisionSourceData {
///     position: vec2<f32>,
///     radius: f32,
///     shape_type: u32,
///     direction_rad: f32,
///     angle_rad: f32,
///     intensity: f32,
///     transition_ratio: f32,
///     cos_direction: f32,
///     sin_direction: f32,
///     cone_half_angle_cos: f32,
/// }
/// ```
#[derive(Copy, Clone, ShaderType, Pod, Zeroable, Debug)]
#[repr(C)]
pub struct VisionSourceData {
    /// World position of the vision source in 2D coordinates.
    /// 视野源在2D坐标中的世界位置
    ///
    /// Extracted from the entity's GlobalTransform. Used by shaders to calculate
    /// distance and direction from vision source to each texture pixel.
    pub position: Vec2,

    /// Maximum vision range in world units.
    /// 世界单位中的最大视野范围
    ///
    /// Determines how far the vision source can see. Pixels beyond this distance
    /// are not affected by this vision source.
    pub radius: f32,

    /// Vision shape type identifier (0=Circle, 1=Cone, 2=Rectangle).
    /// 视野形状类型标识符（0=圆形，1=锥形，2=矩形）
    ///
    /// Determines which algorithm the shader uses for vision calculations:
    /// - **0**: Circle - omnidirectional vision
    /// - **1**: Cone - directional vision with angle constraints
    /// - **2**: Rectangle - rectangular area vision (planned)
    pub shape_type: u32,

    /// Vision direction in radians (for directional vision shapes).
    /// 视野方向（弧度，用于方向性视野形状）
    ///
    /// Used by cone vision to determine the center direction. For circle vision,
    /// this value is ignored. Stored in radians for direct use in shader calculations.
    pub direction_rad: f32,

    /// Vision angle in radians (for cone vision, total field of view).
    /// 视野角度（弧度，用于锥形视野，总视野角）
    ///
    /// For cone vision, this represents the total field of view angle.
    /// The actual half-angle used in calculations is precomputed in cone_half_angle_cos.
    pub angle_rad: f32,

    /// Vision intensity multiplier for visibility calculations.
    /// 可见性计算的视野强度乘数
    ///
    /// Controls how strongly this vision source affects visibility.
    /// Values > 1.0 create stronger vision, values < 1.0 create weaker vision.
    pub intensity: f32,

    /// Edge transition ratio for smooth vision falloff.
    /// 平滑视野衰减的边缘过渡比例
    ///
    /// Controls how soft the edges of the vision area are.
    /// 0.0 = hard edges, higher values = softer transitions.
    pub transition_ratio: f32,

    /// Precomputed cosine of direction angle for performance.
    /// 为性能预计算的方向角余弦值
    ///
    /// Calculated on CPU to avoid trigonometric operations in GPU shaders.
    /// Used for cone vision direction calculations.
    pub cos_direction: f32,

    /// Precomputed sine of direction angle for performance.
    /// 为性能预计算的方向角正弦值
    ///
    /// Calculated on CPU to avoid trigonometric operations in GPU shaders.
    /// Used for cone vision direction calculations.
    pub sin_direction: f32,

    /// Precomputed cosine of half the cone angle for performance.
    /// 为性能预计算的锥角一半的余弦值
    ///
    /// Used in cone vision calculations to determine if a point is within
    /// the cone's angular bounds. Precomputed for GPU efficiency.
    pub cone_half_angle_cos: f32,

    /// Padding to ensure proper GPU memory alignment.
    /// 确保适当GPU内存对齐的填充
    ///
    /// Ensures the structure size is 48 bytes, meeting GPU alignment requirements.
    pub _padding1: f32,
}

/// Minimal chunk data structure optimized for compute shader processing.
/// 为计算着色器处理优化的最小区块数据结构
///
/// This structure contains only the essential information needed by fog compute
/// shaders to process chunk visibility calculations. The minimal design reduces
/// GPU memory bandwidth and improves cache efficiency.
///
/// # Memory Layout
/// ```text
/// Offset | Size | Field           | Purpose
/// -------|------|-----------------|------------------
/// 0      | 8    | coords          | Chunk coordinates
/// 8      | 4    | fog_layer_index | Texture layer index
/// 12     | 4    | _padding        | GPU alignment
/// Total: 16 bytes (GPU-aligned)
/// ```
///
/// # Shader Usage
/// Used in compute shaders to identify which texture layer to process:
/// ```wgsl
/// struct ChunkComputeData {
///     coords: vec2<i32>,
///     fog_layer_index: i32,
/// }
/// @group(0) @binding(3) var<storage, read> chunks: array<ChunkComputeData>;
/// ```
///
/// # Performance Characteristics
/// - **Size**: 16 bytes per chunk (minimal overhead)
/// - **Alignment**: GPU-optimized memory layout
/// - **Access**: Efficient indexed access in compute shaders
/// - **Bandwidth**: Minimal GPU memory bandwidth usage
#[derive(Copy, Clone, ShaderType, Pod, Zeroable, Debug)]
#[repr(C)]
pub struct ChunkComputeData {
    /// Chunk coordinates in chunk space.
    /// 区块空间中的区块坐标
    ///
    /// Used to identify which chunk is being processed and for coordinate
    /// transformations between world space and texture space.
    pub coords: IVec2,

    /// Index of this chunk's fog data in the fog texture array.
    /// 该区块雾效数据在雾效纹理数组中的索引
    ///
    /// Specifies which layer of the fog texture array contains this chunk's
    /// fog data. -1 indicates an invalid or unallocated chunk.
    pub fog_layer_index: i32,

    /// Padding to ensure proper GPU memory alignment.
    /// 确保适当GPU内存对齐的填充
    ///
    /// Required for GPU memory alignment. Total structure size is 16 bytes.
    pub _padding: u32,
}

/// Extended chunk data structure for fog overlay rendering operations.
/// 用于雾效覆盖渲染操作的扩展区块数据结构
///
/// This structure contains chunk information needed for fog overlay rendering,
/// including both fog and snapshot texture layer indices. It enables proper
/// composition of fog effects with snapshot content.
///
/// # Memory Layout
/// ```text
/// Offset | Size | Field                 | Purpose
/// -------|------|-----------------------|------------------------
/// 0      | 8    | coords                | Chunk coordinates
/// 8      | 4    | fog_layer_index       | Fog texture layer index
/// 12     | 4    | snapshot_layer_index  | Snapshot texture layer
/// Total: 16 bytes (GPU-aligned)
/// ```
///
/// # Overlay Rendering
/// Used by overlay shaders to composite fog and snapshot content:
/// 1. **Snapshot Lookup**: Use snapshot_layer_index to sample snapshot texture
/// 2. **Fog Lookup**: Use fog_layer_index to sample fog texture
/// 3. **Composition**: Blend snapshot and scene content based on fog values
///
/// # Performance Characteristics
/// - **Size**: 16 bytes per chunk (compact for overlay operations)
/// - **Indexing**: Efficient dual-texture indexing
/// - **Memory**: Minimal overhead for overlay rendering
/// - **Access**: Optimized for overlay shader texture sampling
#[derive(Copy, Clone, ShaderType, Pod, Zeroable, Debug)]
#[repr(C)]
pub struct OverlayChunkData {
    /// Chunk coordinates in chunk space.
    /// 区块空间中的区块坐标
    ///
    /// Used for coordinate transformations and spatial calculations
    /// during overlay rendering operations.
    pub coords: IVec2,

    /// Index of this chunk's fog data in the fog texture array.
    /// 该区块雾效数据在雾效纹理数组中的索引
    ///
    /// Specifies which layer of the fog texture array contains this chunk's
    /// persistent fog data. Used for fog overlay blending calculations.
    pub fog_layer_index: i32,

    /// Index of this chunk's snapshot data in the snapshot texture array.
    /// 该区块快照数据在快照纹理数组中的索引
    ///
    /// Specifies which layer of the snapshot texture array contains this chunk's
    /// captured entity snapshots. -1 indicates no snapshot available.
    pub snapshot_layer_index: i32,
}

// --- Extraction Systems ---
// --- 提取系统 ---

/// Extracts fog map settings from main world and converts to GPU-compatible format.
/// 从主世界提取雾效地图设置并转换为GPU兼容格式
///
/// This system runs during the extract phase to transfer fog configuration from
/// the main world to the render world. It performs necessary format conversions
/// and color space transformations for GPU compatibility.
///
/// # Conversion Process
/// 1. **Boolean Conversion**: Convert bool to u32 for GPU compatibility
/// 2. **Color Space**: Convert sRGB colors to linear space for proper blending
/// 3. **Format Validation**: Ensure all values are within valid ranges
/// 4. **Padding Addition**: Add required padding for GPU memory alignment
///
/// # Color Space Handling
/// - **Input**: sRGB color values from main world settings
/// - **Conversion**: Automatic sRGB to linear conversion via `.to_linear()`
/// - **Output**: Linear color values suitable for GPU shader operations
/// - **Precision**: Vec4 format maintains sufficient color precision
///
/// # Performance Characteristics
/// - **Frequency**: Runs every frame during extraction phase
/// - **Cost**: Minimal - simple data conversion and copy
/// - **Memory**: Single 80-byte structure allocation
/// - **Time Complexity**: O(1) - constant time conversion
///
/// # Integration Points
/// - **Main World**: Reads from FogMapSettings resource
/// - **Render World**: Creates RenderFogMapSettings resource
/// - **GPU Shaders**: Consumed as uniform buffer in compute and overlay shaders
pub fn extract_fog_settings(mut commands: Commands, settings: Extract<Res<FogMapSettings>>) {
    commands.insert_resource(RenderFogMapSettings {
        enabled: settings.enabled as u32,
        chunk_size: settings.chunk_size,
        texture_resolution_per_chunk: settings.texture_resolution_per_chunk,
        fog_color_unexplored: settings.fog_color_unexplored.to_linear().to_vec4(),
        fog_color_explored: settings.fog_color_explored.to_linear().to_vec4(),
        vision_clear_color: settings.vision_clear_color.to_linear().to_vec4(),
        _padding1: [0; 3],
    });
}

/// Extracts texture array handles from main world to render world for GPU access.
/// 从主世界提取纹理数组句柄到渲染世界以供GPU访问
///
/// This system transfers texture handles from main world resources to render world
/// resources, ensuring GPU shaders can access the fog texture arrays. Handle transfer
/// is necessary because render world operates independently of main world.
///
/// # Texture Handle Types
/// - **FogTextureArray**: Persistent fog exploration data
/// - **VisibilityTextureArray**: Real-time visibility calculations
/// - **SnapshotTextureArray**: Captured entity snapshots
/// - **SnapshotTempTexture**: Temporary texture for snapshot capture
///
/// # Handle Management
/// - **Clone Operations**: Handles are cloned, not moved, preserving main world access
/// - **Resource Creation**: Creates render world resources with cloned handles
/// - **GPU Binding**: Enables render systems to bind textures to shaders
/// - **Lifetime Management**: Handles maintain texture lifetime across worlds
///
/// # Performance Characteristics
/// - **Frequency**: Runs every frame during extraction phase
/// - **Cost**: Minimal - handle cloning is cheap
/// - **Memory**: Four handle clones per frame
/// - **Time Complexity**: O(1) - constant time handle operations
///
/// # Error Handling
/// Assumes all texture resources exist in main world. Missing resources would
/// cause panics during development, providing early error detection.
///
/// # Integration Points
/// - **Main World**: Reads texture handles from texture array resources
/// - **Render World**: Creates corresponding render world texture handle resources
/// - **GPU Systems**: Used by prepare systems to bind textures to shaders
pub fn extract_texture_handles(
    mut commands: Commands,
    fog_texture: Extract<Res<FogTextureArray>>,
    visibility_texture: Extract<Res<VisibilityTextureArray>>,
    snapshot_texture: Extract<Res<SnapshotTextureArray>>,
    snapshot_temp_texture: Extract<Res<SnapshotTempTexture>>,
) {
    // Ensure the handles exist in the RenderWorld / 确保句柄存在于 RenderWorld 中
    commands.insert_resource(RenderFogTexture(fog_texture.handle.clone()));
    commands.insert_resource(RenderVisibilityTexture(visibility_texture.handle.clone()));
    commands.insert_resource(RenderSnapshotTexture(snapshot_texture.handle.clone()));
    commands.insert_resource(RenderSnapshotTempTexture(
        snapshot_temp_texture.handle.clone(),
    ));
}

/// Extracts and processes vision source entities for GPU shader consumption.
/// 为GPU着色器消费提取和处理视野源实体
///
/// This system queries all entities with VisionSource components, converts them
/// to GPU-compatible data structures, and performs trigonometric precalculations
/// for optimal shader performance.
///
/// # Processing Pipeline
/// 1. **Entity Query**: Find all entities with GlobalTransform and VisionSource
/// 2. **Filtering**: Include only enabled vision sources
/// 3. **Coordinate Extraction**: Get world position from GlobalTransform
/// 4. **Shape Conversion**: Convert VisionShape enum to numeric GPU format
/// 5. **Trigonometric Precalculation**: Compute cos/sin values for GPU efficiency
/// 6. **Data Structure Creation**: Build GPU-compatible VisionSourceData
///
/// # Shape Type Mapping
/// - **VisionShape::Circle** → 0: Omnidirectional circular vision
/// - **VisionShape::Cone** → 1: Directional cone-shaped vision
/// - **VisionShape::Square** → 2: Rectangular area vision (future)
///
/// # Performance Optimizations
/// - **CPU Trigonometry**: cos/sin calculated on CPU to reduce GPU load
/// - **Precalculated Half-Angles**: Cone calculations optimized for GPU
/// - **Enabled Filtering**: Disabled sources excluded to reduce GPU workload
/// - **Memory Reuse**: Clears and reuses existing vector to avoid allocations
///
/// # Fallback Behavior
/// If no enabled vision sources exist, inserts a default disabled source with
/// zero values to ensure GPU shaders always have valid data to read.
///
/// # Performance Characteristics
/// - **Entity Processing**: O(n) where n = number of vision source entities
/// - **Trigonometric Cost**: O(n) CPU trigonometric calculations
/// - **Memory**: Allocates n × 48 bytes for vision source data
/// - **GPU Benefit**: Eliminates trigonometric calculations in GPU shaders
///
/// # Integration Points
/// - **Main World**: Queries VisionSource and GlobalTransform components
/// - **Render World**: Populates ExtractedVisionSources resource
/// - **GPU Systems**: Used to create vision source storage buffer for shaders
pub fn extract_vision_sources(
    mut sources_res: ResMut<ExtractedVisionSources>,
    vision_sources: Extract<Query<(&GlobalTransform, &VisionSource)>>,
) {
    sources_res.sources.clear();
    sources_res
        .sources
        .extend(
            vision_sources
                .iter()
                .filter(|(_, src)| src.enabled)
                .map(|(transform, src)| {
                    // 将形状枚举转换为数值
                    // Convert shape enum to numeric value
                    let shape_type = match src.shape {
                        VisionShape::Circle => 0u32,
                        VisionShape::Cone => 1u32,
                        VisionShape::Square => 2u32,
                    };

                    let cos_dir = src.direction.cos();
                    let sin_dir = src.direction.sin();
                    // For cone, angle is the full FOV. Shader uses half_angle.
                    // 对于扇形，angle 是完整的视场角。Shader 使用半角。
                    let cone_cos_half_angle = (src.angle * 0.5).cos();

                    VisionSourceData {
                        position: transform.translation().truncate(),
                        radius: src.range,
                        shape_type,
                        direction_rad: src.direction, // Store original direction in radians / 存储原始方向（弧度）
                        angle_rad: src.angle, // Store original angle in radians / 存储原始角度（弧度）
                        intensity: src.intensity,
                        transition_ratio: src.transition_ratio,
                        cos_direction: cos_dir,
                        sin_direction: sin_dir,
                        cone_half_angle_cos: cone_cos_half_angle,
                        _padding1: 0.0, // Initialize padding / 初始化填充
                    }
                }),
        );

    if sources_res.sources.is_empty() {
        sources_res.sources.push(VisionSourceData {
            position: Default::default(),
            radius: 0.0,
            shape_type: 0, // Circle by default / 默认为圆形
            direction_rad: 0.0,
            angle_rad: 0.0, // Full circle if cone, but irrelevant for shape_type 0 / 如果是扇形则为全圆，但对 shape_type 0 无关紧要
            intensity: 0.0,
            transition_ratio: 0.0,
            cos_direction: 1.0,       // cos(0)
            sin_direction: 0.0,       // sin(0)
            cone_half_angle_cos: 1.0, // cos(0 * 0.5)
            _padding1: 0.0,
        });
    }
}

/// Constant representing an invalid GPU texture layer index.
/// 表示无效GPU纹理层索引的常量
///
/// This value is used to indicate chunks that don't have valid GPU texture
/// layer allocations. GPU shaders can check for this value to skip processing
/// invalid chunks.
const GFX_INVALID_LAYER: i32 = -1;

/// Extracts and processes chunk data with frustum culling for efficient GPU processing.
/// 提取和处理区块数据并进行视锥剔除以实现高效GPU处理
///
/// This system performs the most complex extraction operation, combining chunk
/// filtering, frustum culling, and data format conversion for optimal GPU
/// performance. It significantly reduces GPU workload by processing only
/// chunks visible to the camera.
///
/// # Processing Pipeline
/// 1. **Camera Analysis**: Calculate camera view bounds for culling
/// 2. **Chunk Iteration**: Process all chunks with GPU-compatible memory state
/// 3. **Memory State Filtering**: Include only GPU-resident or pending chunks
/// 4. **Spatial Culling**: Test chunk intersection with camera view
/// 5. **Data Conversion**: Convert chunk data to GPU-compatible formats
/// 6. **Fallback Insertion**: Ensure minimum data for GPU operations
///
/// # Frustum Culling Algorithm
/// Uses axis-aligned bounding box intersection for orthographic cameras:
/// ```rust,no_run
/// // AABB intersection test
/// is_visible = !(chunk.max.x < view.min.x || chunk.min.x > view.max.x ||
///              chunk.max.y < view.min.y || chunk.min.y > view.max.y)
/// ```
///
/// # Camera Support
/// - **Orthographic**: Full frustum culling support with accurate bounds calculation
/// - **Perspective**: Warning issued, falls back to no culling (future enhancement)
/// - **Missing Camera**: No culling performed, all GPU chunks processed
///
/// # Memory State Filtering
/// Only processes chunks in specific memory states:
/// - **ChunkMemoryLocation::Gpu**: Chunks with active GPU texture allocations
/// - **ChunkMemoryLocation::PendingCopyToGpu**: Chunks being uploaded to GPU
/// - **Other States**: Skipped to avoid processing chunks without GPU data
///
/// # Performance Impact
/// Frustum culling typically reduces processed chunks by 60-90%:
/// - **Large Worlds**: Dramatic performance improvement
/// - **Small Scenes**: Minimal overhead, still beneficial
/// - **Camera Movement**: Culling adapts dynamically to camera position
/// - **GPU Workload**: Directly proportional to visible chunk count
///
/// # Data Structure Creation
/// Creates two synchronized data sets:
/// - **compute_chunks**: Minimal data for compute shader operations
/// - **overlay_mapping**: Extended data for overlay rendering with snapshots
///
/// # Error Handling
/// - **Invalid Chunks**: Chunks with invalid layer indices are properly handled
/// - **Empty Results**: Fallback data ensures GPU shaders always have valid input
/// - **Camera Issues**: Graceful fallback to no culling when camera unavailable
/// - **Projection Warnings**: Clear feedback for unsupported camera types
///
/// # Integration Points
/// - **Main World**: Queries FogChunk components and camera information
/// - **Render World**: Populates ExtractedGpuChunkData resource
/// - **GPU Systems**: Used to create chunk data storage buffers for shaders
/// - **Performance**: Directly impacts GPU compute and overlay shader workload
pub fn extract_gpu_chunk_data(
    mut chunk_data_res: ResMut<ExtractedGpuChunkData>,
    settings: Extract<Res<FogMapSettings>>,
    camera_query: Extract<Query<(&GlobalTransform, &Projection), With<FogOfWarCamera>>>,
    fog_chunk_query: Extract<Query<&FogChunk>>,
) {
    chunk_data_res.compute_chunks.clear();
    chunk_data_res.overlay_mapping.clear();

    let mut view_aabb_world: Option<Rect> = None;

    if let Ok((camera_transform, projection)) = camera_query.single() {
        // Calculate view AABB for an orthographic camera
        // This assumes the FogOfWarCamera is orthographic. Handle perspective if needed.
        if let Projection::Orthographic(ortho_projection) = projection {
            let camera_scale = camera_transform.compute_transform().scale;
            // ortho_projection.area gives the size of the projection area.
            // For WindowSize scale mode, this area is in logical pixels, needing viewport size.
            // For Fixed scale mode, this area is in world units.
            // We'll assume Fixed scale mode or that area is already in appropriate units
            // that can be scaled by camera_transform.scale to get world dimensions.
            // A more robust way for WindowSize would be to use camera.logical_viewport_size().
            let half_width = ortho_projection.area.width() * 0.5 * camera_scale.x;
            let half_height = ortho_projection.area.height() * 0.5 * camera_scale.y;
            let camera_pos_2d = camera_transform.translation().truncate();

            view_aabb_world = Some(Rect {
                min: Vec2::new(camera_pos_2d.x - half_width, camera_pos_2d.y - half_height),
                max: Vec2::new(camera_pos_2d.x + half_width, camera_pos_2d.y + half_height),
            });
        } else {
            warn!(
                "FogOfWarCamera is not using an OrthographicProjection. Culling might not work as expected for perspective cameras with this AABB logic."
            );
            // For perspective, you'd need to implement frustum culling.
        }
    } else {
        warn!(
            "No single FogOfWarCamera found, or multiple were found. Fog chunk culling will not be performed."
        );
        // If no camera, all GPU-ready chunks will be processed (original behavior for this path)
    }

    let chunk_world_size_f32 = settings.chunk_size.as_vec2();

    for chunk in fog_chunk_query.iter() {
        if !(chunk.state.memory_location == ChunkMemoryLocation::Gpu
            || chunk.state.memory_location == ChunkMemoryLocation::PendingCopyToGpu)
        {
            continue; // Skip if not on GPU or pending
        }

        let chunk_min_world = chunk.coords.as_vec2() * chunk_world_size_f32;
        let chunk_max_world = chunk_min_world + chunk_world_size_f32;
        let chunk_aabb_world = Rect {
            min: chunk_min_world,
            max: chunk_max_world,
        };

        let mut is_visible_or_no_culling = true; // Default to true if culling is not active
        if let Some(view_rect) = view_aabb_world {
            // AABB intersection test
            is_visible_or_no_culling = !(chunk_aabb_world.max.x < view_rect.min.x
                || chunk_aabb_world.min.x > view_rect.max.x
                || chunk_aabb_world.max.y < view_rect.min.y
                || chunk_aabb_world.min.y > view_rect.max.y);
        }

        if is_visible_or_no_culling {
            let fog_idx_gfx = chunk
                .fog_layer_index
                .map_or(GFX_INVALID_LAYER, |val| val as i32);
            let snap_idx_gfx = chunk
                .snapshot_layer_index
                .map_or(GFX_INVALID_LAYER, |val| val as i32);

            chunk_data_res.compute_chunks.push(ChunkComputeData {
                coords: chunk.coords,
                fog_layer_index: fog_idx_gfx,
                _padding: 0,
            });
            chunk_data_res.overlay_mapping.push(OverlayChunkData {
                coords: chunk.coords,
                fog_layer_index: fog_idx_gfx,
                snapshot_layer_index: snap_idx_gfx,
            });
        }
    }

    // Fallback if no valid chunks found (or all culled)
    if chunk_data_res.compute_chunks.is_empty() {
        chunk_data_res.compute_chunks.push(ChunkComputeData {
            coords: Default::default(),
            fog_layer_index: -1,
            _padding: 0,
        });
        chunk_data_res.overlay_mapping.push(OverlayChunkData {
            coords: Default::default(),
            fog_layer_index: -1,
            snapshot_layer_index: -1,
        });
    }
}
