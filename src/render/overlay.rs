//! Fog overlay rendering system that composites fog effects over the main scene.
//! 在主场景上合成雾效的雾效覆盖渲染系统
//!
//! This module implements the final stage of fog rendering, where fog textures are
//! composited over the main scene to create the visual fog of war effect. It handles
//! texture sampling, color blending, and final composition.
//!
//! # Overlay Rendering Pipeline
//!
//! ## Rendering Process
//! The overlay system executes as the final step in fog rendering:
//! ```text
//! [Scene Rendering] → [Fog Compute] → [Fog Overlay] → [Final Output]
//!        ↓                ↓              ↓               ↓
//! Base Scene Content → GPU Compute → Texture Sampling → Composited Result
//! Lighting/Materials    Visibility    Fog/Snapshots     Final Frame
//! ```
//!
//! ## Fullscreen Rendering
//! - **Vertex Stage**: Uses Bevy's fullscreen vertex shader for screen-aligned triangle
//! - **Fragment Stage**: Custom fog overlay shader handles per-pixel fog compositing
//! - **Render Target**: Draws directly to the main view target
//! - **Blend Mode**: Alpha blending for proper fog transparency effects
//!
//! # Texture Composition
//!
//! ## Multi-Texture Sampling
//! The overlay shader samples three main texture arrays:
//! - **Visibility Texture**: Real-time visibility data from compute shaders
//! - **Fog Texture**: Persistent exploration data accumulated over time
//! - **Snapshot Texture**: Captured entity snapshots for explored areas
//!
//! ## Coordinate Transformation
//! Converts screen coordinates to chunk texture coordinates:
//! ```glsl
//! // Screen space to world space
//! world_pos = screen_to_world(screen_coords, view_matrix)
//!
//! // World space to chunk coordinates  
//! chunk_coords = floor(world_pos / chunk_size)
//! chunk_local = mod(world_pos, chunk_size) / chunk_size
//!
//! // Sample from texture array
//! fog_value = texture(fog_array, vec3(chunk_local, layer_index))
//! ```
//!
//! # Performance Characteristics
//!
//! ## GPU Efficiency
//! - **Fullscreen Pass**: Single fullscreen triangle for minimal vertex processing
//! - **Texture Cache**: Efficient texture array sampling with GPU cache optimization
//! - **Fragment Shading**: Optimized per-pixel operations with early fragment culling
//! - **Bandwidth**: Minimized memory bandwidth through efficient texture formats
//!
//! ## Rendering Cost
//! - **Resolution Dependent**: O(screen_width × screen_height) fragment operations
//! - **Texture Samples**: 3-6 texture samples per pixel depending on fog configuration
//! - **Memory Access**: Coalesced texture array access patterns
//! - **Fill Rate**: Limited by GPU fill rate for fullscreen effects
//!
//! # Shader Resources
//!
//! ## Bind Group Layout (Binding Index)
//! - **0**: View uniform buffer (ViewUniform) - Camera and projection data
//! - **1**: Texture sampler (Linear filtering) - Shared sampler for all textures
//! - **2**: Visibility texture array (2D Array) - Real-time visibility data
//! - **3**: Fog texture array (2D Array) - Persistent exploration data
//! - **4**: Snapshot texture array (2D Array) - Captured entity snapshots
//! - **5**: Fog settings uniform (RenderFogMapSettings) - Global fog configuration
//! - **6**: Chunk mapping buffer (Storage) - Chunk coordinate to texture layer mapping
//!
//! ## Texture Format Requirements
//! - **Visibility/Fog**: R8Unorm format (1 byte per pixel) for memory efficiency
//! - **Snapshots**: RGBA8 format (4 bytes per pixel) for full color capture
//! - **Filtering**: Linear filtering for smooth fog transitions
//! - **Address Mode**: Clamp to edge to avoid sampling artifacts
//!
//! # Integration Points
//!
//! ## Render Graph Position
//! Executes as the final stage in the fog rendering pipeline:
//! ```text
//! MainTransparentPass → SnapshotNode → FogComputeNode → FogOverlayNode → EndMainPass
//! ```
//!
//! ## View Node Implementation
//! - **Per-View Execution**: Runs once per camera view
//! - **View Uniforms**: Accesses camera-specific uniform data
//! - **Dynamic Offsets**: Uses view uniform offsets for multi-view support
//! - **Target Selection**: Renders to appropriate view target
//!
//! # Error Handling
//!
//! ## Graceful Fallbacks
//! - **Missing Textures**: Uses fallback textures when fog textures unavailable
//! - **Pipeline Compilation**: Skips rendering when shaders not compiled
//! - **Buffer Availability**: Waits for GPU buffer preparation
//! - **Snapshot Cameras**: Excludes snapshot cameras from fog overlay rendering
//!
//! # Future Optimizations
//!
//! ## Potential Enhancements
//! - **Temporal Upsampling**: Render fog at lower resolution and upscale
//! - **Multi-Resolution**: Different fog detail levels based on distance
//! - **Compute-Based Overlay**: Move overlay to compute shaders for efficiency
//! - **Tile-Based Rendering**: Process fog in screen-space tiles for better cache locality

use super::RenderFogMapSettings;
use super::extract::{
    OverlayChunkData, RenderFogTexture, RenderSnapshotTexture, RenderVisibilityTexture,
};
use super::prepare::{FogUniforms, OverlayChunkMappingBuffer};
use crate::snapshot::SnapshotCamera;
use bevy::core_pipeline::fullscreen_vertex_shader::FULLSCREEN_SHADER_HANDLE;
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::lifetimeless::Read;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{NodeRunError, RenderGraphContext, RenderLabel, ViewNode};
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only, texture_2d_array, uniform_buffer,
};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::render::view::{ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms};

/// Path to the WGSL fog overlay shader that implements final fog compositing.
/// 实现最终雾效合成的WGSL雾效覆盖着色器的路径
///
/// This shader handles the final stage of fog rendering, compositing fog textures
/// over the main scene. It performs texture sampling, coordinate transformations,
/// and color blending to create the final fog of war visual effect.
///
/// # Shader Capabilities
/// - **Multi-texture Sampling**: Samples visibility, fog, and snapshot textures
/// - **Coordinate Transformation**: Converts screen space to chunk texture coordinates
/// - **Color Blending**: Combines fog colors with scene and snapshot content
/// - **Performance Optimization**: Efficient per-pixel operations with GPU optimization
const SHADER_ASSET_PATH: &str = "shaders/fog_overlay.wgsl";

/// Render graph label for the fog overlay rendering node.
/// 雾效覆盖渲染节点的渲染图标签
///
/// This label identifies the fog overlay node within Bevy's render graph,
/// enabling proper ordering and dependency management. The overlay node
/// executes as the final stage in fog rendering, after compute operations.
///
/// # Render Graph Integration
/// Used to establish render graph dependencies:
/// ```rust
/// render_app.add_render_graph_edges(
///     Core2d,
///     (
///         Node2d::MainTransparentPass,
///         SnapshotNodeLabel,
///         FogComputeNodeLabel,
///         FogOverlayNodeLabel,  // This label
///         Node2d::EndMainPass,
///     ),
/// );
/// ```
///
/// # Label Properties
/// - **Unique**: Distinguishes overlay node from other render nodes
/// - **Hashable**: Enables efficient render graph operations
/// - **Debug**: Provides debugging information for render graph inspection
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct FogOverlayNodeLabel;

/// Render graph node that performs fog overlay rendering as a fullscreen effect.
/// 执行雾效覆盖渲染作为全屏效果的渲染图节点
///
/// This node implements the final stage of fog rendering, compositing fog textures
/// over the main scene to create the visual fog of war effect. It operates as a
/// fullscreen post-processing effect using GPU fragment shaders.
///
/// # Rendering Strategy
/// - **Fullscreen Triangle**: Uses a single screen-aligned triangle for optimal GPU utilization
/// - **Fragment-Based**: All fog logic handled in fragment shader for per-pixel control
/// - **Multi-Texture Sampling**: Accesses visibility, fog, and snapshot texture arrays
/// - **Alpha Blending**: Proper transparency blending with scene content
///
/// # Performance Characteristics
/// - **Resolution Dependent**: O(screen_width × screen_height) fragment operations
/// - **GPU Bound**: Limited by fragment shading performance and texture bandwidth
/// - **Cache Efficient**: Optimized texture array access patterns
/// - **Fill Rate**: Bounded by GPU fill rate for fullscreen effects
///
/// # Integration Points
/// - **View Node**: Runs once per camera view with view-specific uniforms
/// - **Render Graph**: Executes after fog compute operations
/// - **Resource Access**: Uses prepared GPU buffers and texture arrays
/// - **Fallback Support**: Graceful handling of missing resources
#[derive(Default)]
pub struct FogOverlayNode;

/// GPU render pipeline resource for fog overlay shader operations.
/// 雾效覆盖着色器操作的GPU渲染管线资源
///
/// This resource encapsulates the compiled GPU render pipeline, bind group layout,
/// and associated resources needed for fog overlay rendering. It's created once
/// during application startup and reused for all fog overlay operations.
///
/// # Pipeline Components
/// - **Render Pipeline**: Compiled GPU pipeline with vertex and fragment stages
/// - **Bind Group Layout**: Resource binding schema for shader inputs
/// - **Texture Sampler**: Shared sampler for all texture array operations
/// - **Pipeline State**: Complete GPU pipeline state for overlay rendering
///
/// # Resource Binding Schema
/// The pipeline expects resources bound in this specific order:
/// ```wgsl
/// @group(0) @binding(0) var<uniform> view: ViewUniform;
/// @group(0) @binding(1) var sampler: sampler;
/// @group(0) @binding(2) var visibility_texture: texture_2d_array<f32>;
/// @group(0) @binding(3) var fog_texture: texture_2d_array<f32>;
/// @group(0) @binding(4) var snapshot_texture: texture_2d_array<f32>;
/// @group(0) @binding(5) var<uniform> settings: RenderFogMapSettings;
/// @group(0) @binding(6) var<storage, read> chunks: array<OverlayChunkData>;
/// ```
///
/// # Sampler Configuration
/// - **Filtering**: Linear filtering for smooth fog transitions
/// - **Address Mode**: Clamp to edge to prevent sampling artifacts
/// - **Mip Mapping**: Linear mip mapping for texture level transitions
/// - **Anisotropy**: No anisotropic filtering (not needed for fog textures)
///
/// # Performance Characteristics
/// - **Compilation Cost**: One-time render pipeline compilation during startup
/// - **Memory Overhead**: Minimal pipeline state storage
/// - **Reusability**: Single pipeline used for all fog overlay operations
/// - **GPU Efficiency**: Optimized bind group layout for texture array access
///
/// # Alpha Blending Configuration
/// Uses standard alpha blending for proper fog transparency:
/// - **Source Factor**: SrcAlpha - Use fog alpha for source contribution
/// - **Destination Factor**: OneMinusSrcAlpha - Preserve scene based on fog transparency
/// - **Operation**: Add - Standard additive blending for transparency
/// - **Write Mask**: All channels - Full RGBA output control
#[derive(Resource)]
pub struct FogOverlayPipeline {
    /// Bind group layout defining resource binding schema for the overlay shader.
    /// 定义覆盖着色器资源绑定模式的绑定组布局
    ///
    /// This layout specifies how CPU resources (textures, buffers, uniforms)
    /// are bound to GPU shader inputs. Used to create bind groups for overlay rendering.
    layout: BindGroupLayout,

    /// Shared texture sampler for all fog texture array operations.
    /// 用于所有雾效纹理数组操作的共享纹理采样器
    ///
    /// Configured with linear filtering and clamp addressing for optimal fog
    /// texture sampling. Reused across all texture array bindings for efficiency.
    sampler: Sampler,

    /// Cached render pipeline identifier for efficient pipeline retrieval.
    /// 用于高效管线检索的缓存渲染管线标识符
    ///
    /// Used with Bevy's pipeline cache to retrieve the compiled render pipeline.
    /// The pipeline may not be immediately available if shader compilation is pending.
    pipeline_id: CachedRenderPipelineId,
}

/// Initializes the fog overlay pipeline from world resources during application startup.
/// 在应用程序启动期间从世界资源初始化雾效覆盖管线
///
/// This implementation creates the complete GPU render pipeline including shader
/// compilation, bind group layout creation, sampler setup, and pipeline state
/// configuration. It's called once during application initialization.
impl FromWorld for FogOverlayPipeline {
    /// Creates a new fog overlay pipeline from world resources.
    /// 从世界资源创建新的雾效覆盖管线
    ///
    /// This method is called by Bevy's resource initialization system to create
    /// the render pipeline. It sets up all necessary GPU resources for fog
    /// overlay rendering operations.
    ///
    /// # Resource Dependencies
    /// Requires these world resources to be available:
    /// - **PipelineCache**: For render pipeline compilation and caching
    /// - **RenderDevice**: For GPU resource creation and bind group layouts
    /// - **Asset System**: For loading the WGSL overlay shader file
    ///
    /// # GPU Resource Creation
    /// Creates several GPU resources:
    /// - **Bind Group Layout**: Defines shader resource binding schema
    /// - **Texture Sampler**: Linear filtering sampler for texture arrays
    /// - **Pipeline Descriptor**: Specifies render pipeline configuration
    /// - **Shader Assets**: Loads WGSL overlay shader and fullscreen vertex shader
    ///
    /// # Pipeline Configuration
    /// Sets up render pipeline with:
    /// - **Vertex Stage**: Bevy's fullscreen vertex shader for screen-aligned triangle
    /// - **Fragment Stage**: Custom fog overlay shader for per-pixel compositing
    /// - **Blend State**: Alpha blending for proper fog transparency
    /// - **Color Target**: Standard HDR-capable color format with full write mask
    ///
    /// # Return Value
    /// Returns a configured FogOverlayPipeline with:
    /// - **layout**: Bind group layout for resource binding
    /// - **sampler**: Configured texture sampler for filtering
    /// - **pipeline_id**: Cached pipeline ID for runtime retrieval
    ///
    /// # Time Complexity
    /// O(1) for resource access and pipeline queuing, but shader compilation
    /// happens asynchronously and may take additional time.
    ///
    /// # Performance Considerations
    /// - **One-time Cost**: Expensive operation performed only at startup
    /// - **GPU Validation**: GPU driver validates shader and pipeline state
    /// - **Memory Allocation**: Allocates GPU pipeline state objects
    /// - **Compilation Time**: Shader compilation may take several milliseconds
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();

        // Create bind group layout defining shader resource binding schema
        // 创建定义着色器资源绑定模式的绑定组布局
        let layout = render_device.create_bind_group_layout(
            "fog_overlay_bind_group_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    uniform_buffer::<ViewUniform>(true), // 0: Camera view uniforms (dynamic offset)
                    sampler(SamplerBindingType::Filtering), // 1: Texture sampler for filtering
                    texture_2d_array(TextureSampleType::Float { filterable: true }), // 2: Visibility texture array
                    texture_2d_array(TextureSampleType::Float { filterable: true }), // 3: Fog texture array
                    texture_2d_array(TextureSampleType::Float { filterable: true }), // 4: Snapshot texture array
                    uniform_buffer::<RenderFogMapSettings>(false), // 5: Fog settings uniform
                    storage_buffer_read_only::<OverlayChunkData>(false), // 6: Chunk mapping storage buffer
                ),
            ),
        );

        // Create linear filtering sampler for smooth fog texture transitions
        // 创建用于平滑雾效纹理过渡的线性过滤采样器
        let sampler = render_device.create_sampler(&SamplerDescriptor {
            label: Some("fog_overlay_sampler"),
            mag_filter: FilterMode::Linear, // Linear magnification for smooth scaling
            min_filter: FilterMode::Linear, // Linear minification for smooth scaling
            mipmap_filter: FilterMode::Linear, // Linear mip mapping for level transitions
            address_mode_u: AddressMode::ClampToEdge, // Clamp U axis to prevent sampling artifacts
            address_mode_v: AddressMode::ClampToEdge, // Clamp V axis to prevent sampling artifacts
            address_mode_w: AddressMode::ClampToEdge, // Clamp W axis for texture array layers
            ..Default::default()
        });

        // Load fog overlay fragment shader asset
        // 加载雾效覆盖片段着色器资源
        let shader = world.load_asset(SHADER_ASSET_PATH);

        // Queue render pipeline for compilation with complete configuration
        // 排队渲染管线以进行完整配置的编译
        let pipeline_id =
            world
                .resource_mut::<PipelineCache>()
                .queue_render_pipeline(RenderPipelineDescriptor {
                    label: Some("fog_overlay_pipeline_init".into()), // Pipeline identifier for debugging
                    layout: vec![layout.clone()], // Use the bind group layout created above
                    vertex: VertexState {
                        shader: FULLSCREEN_SHADER_HANDLE, // Bevy's built-in fullscreen vertex shader
                        shader_defs: vec![],              // No shader preprocessor definitions
                        entry_point: "fullscreen_vertex_shader".into(), // Standard vertex entry point
                        buffers: vec![], // No vertex buffers (fullscreen triangle)
                    },
                    fragment: Some(FragmentState {
                        shader,                         // Custom fog overlay fragment shader
                        shader_defs: vec![],            // No shader preprocessor definitions
                        entry_point: "fragment".into(), // Fragment shader entry point
                        targets: vec![Some(ColorTargetState {
                            format: TextureFormat::bevy_default(), // Use Bevy's default HDR format
                            blend: Some(BlendState::ALPHA_BLENDING), // Standard alpha blending for transparency
                            write_mask: ColorWrites::ALL,            // Write to all RGBA channels
                        })],
                    }),
                    primitive: PrimitiveState::default(), // Default primitive settings (triangle list)
                    depth_stencil: None,                  // No depth testing for fullscreen overlay
                    multisample: MultisampleState::default(), // Default multisampling settings
                    push_constant_ranges: vec![],         // No push constants used
                    zero_initialize_workgroup_memory: false, // Not applicable for render pipelines
                });

        // Return configured pipeline with all components
        // 返回包含所有组件的配置管线
        FogOverlayPipeline {
            layout,      // Bind group layout for resource binding
            sampler,     // Texture sampler for filtering
            pipeline_id, // Cached pipeline ID for runtime retrieval
        }
    }
}

/// Implements the view node interface for fog overlay rendering operations.
/// 为雾效覆盖渲染操作实现视图节点接口
///
/// This implementation defines how the fog overlay node executes within Bevy's
/// render graph for each camera view. It handles resource gathering, bind group
/// creation, and fullscreen rendering operations.
impl ViewNode for FogOverlayNode {
    /// Query type for view-specific data needed by the overlay node.
    /// 覆盖节点所需的视图特定数据的查询类型
    ///
    /// - **ViewTarget**: Provides access to the view's render target for output
    /// - **ViewUniformOffset**: Provides dynamic offset for view uniform buffer access
    type ViewQuery = (Read<ViewTarget>, Read<ViewUniformOffset>);

    /// Executes fog overlay rendering for a specific camera view.
    /// 为特定相机视图执行雾效覆盖渲染
    ///
    /// This method is called by Bevy's render graph system for each camera view
    /// that requires fog overlay rendering. It performs fullscreen fog compositing
    /// by sampling fog textures and blending them over the main scene.
    ///
    /// # Execution Flow
    /// 1. **Camera Validation**: Skip snapshot cameras that don't need fog overlay
    /// 2. **Resource Gathering**: Collect all required GPU resources and buffers
    /// 3. **Pipeline Validation**: Ensure render pipeline is compiled and ready
    /// 4. **Buffer Validation**: Verify all uniform and storage buffers are prepared
    /// 5. **Texture Access**: Get texture views for fog, visibility, and snapshot arrays
    /// 6. **Bind Group Creation**: Create view-specific bind group with all resources
    /// 7. **Render Pass**: Execute fullscreen triangle rendering with fog shader
    ///
    /// # Resource Dependencies
    /// - **FogOverlayPipeline**: Compiled render pipeline and bind group layout
    /// - **FogUniforms**: GPU uniform buffer with fog settings
    /// - **OverlayChunkMappingBuffer**: Storage buffer with chunk coordinate mappings
    /// - **Texture Arrays**: Visibility, fog, and snapshot texture arrays
    /// - **View Uniforms**: Camera-specific uniform data with dynamic offsets
    ///
    /// # Rendering Technique
    /// Uses a fullscreen triangle approach for optimal GPU utilization:
    /// - **Vertex Stage**: Bevy's fullscreen vertex shader generates screen positions
    /// - **Fragment Stage**: Custom fog shader samples textures and composites fog
    /// - **Alpha Blending**: Standard alpha blending for proper fog transparency
    /// - **Screen Coverage**: Single triangle covers entire screen efficiently
    ///
    /// # Performance Characteristics
    /// - **Resolution Dependent**: O(screen_width × screen_height) fragment operations
    /// - **Texture Bandwidth**: 3-6 texture samples per pixel for fog data
    /// - **Fill Rate Bound**: Limited by GPU fragment processing and texture cache
    /// - **Memory Access**: Coalesced texture array access for cache efficiency
    ///
    /// # Error Handling
    /// Returns Ok(()) in all cases to maintain render graph stability:
    /// - **Snapshot Camera**: Skips fog overlay for snapshot capture cameras
    /// - **Missing Pipeline**: Waits for shader compilation without blocking
    /// - **Missing Buffers**: Skips rendering when GPU buffers not ready
    /// - **Missing Textures**: Uses fallback textures for graceful degradation
    ///
    /// # Bind Group Configuration
    /// Creates per-view bind group with:
    /// - **Dynamic Offset**: View uniform offset for multi-view support
    /// - **Texture Arrays**: All three fog texture arrays (visibility, fog, snapshot)
    /// - **Uniform Buffers**: Fog settings and view-specific data
    /// - **Storage Buffer**: Chunk mapping data for coordinate transformations
    ///
    /// # Integration Points
    /// - **Render Graph**: Executes after compute operations in fog pipeline
    /// - **View System**: Runs once per camera view with view-specific data
    /// - **Resource System**: Uses prepared GPU buffers and texture arrays
    /// - **Pipeline Cache**: Retrieves compiled render pipeline from cache
    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, view_uniform_offset): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        // Get the current view entity for camera-specific processing
        // 获取当前视图实体以进行相机特定处理
        let view_entity = graph.view_entity();

        // Skip fog overlay rendering for snapshot cameras
        // 为快照相机跳过雾效覆盖渲染
        if world.get::<SnapshotCamera>(view_entity).is_some() {
            return Ok(()); // Snapshot cameras don't need fog overlay
        }

        // Gather required pipeline and cache resources
        // 收集所需的管线和缓存资源
        let overlay_pipeline = world.resource::<FogOverlayPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();

        // Retrieve compiled render pipeline from cache
        // 从缓存中检索编译的渲染管线
        let Some(pipeline) = pipeline_cache.get_render_pipeline(overlay_pipeline.pipeline_id)
        else {
            // Pipeline not compiled yet, skip this frame gracefully
            // 管线尚未编译，优雅地跳过此帧
            return Ok(());
        };

        // Gather all required GPU resources for fog overlay rendering
        // 收集雾效覆盖渲染所需的所有GPU资源
        let fog_uniforms = world.resource::<FogUniforms>();
        let overlay_chunk_buffer = world.resource::<OverlayChunkMappingBuffer>();
        let visibility_texture = world.resource::<RenderVisibilityTexture>();
        let fog_texture = world.resource::<RenderFogTexture>();
        let snapshot_texture = world.resource::<RenderSnapshotTexture>();
        let images = world.resource::<RenderAssets<GpuImage>>();
        let fallback_image = world.resource::<FallbackImage>();
        let view_uniforms = world.resource::<ViewUniforms>();

        // Validate that all required GPU buffers are prepared and ready
        // 验证所有必需的GPU缓冲区都已准备就绪
        let (
            Some(uniform_buf),          // Fog settings uniform buffer
            Some(mapping_buf),          // Chunk mapping storage buffer
            Some(view_uniform_binding), // View uniform buffer binding
        ) = (
            fog_uniforms.buffer.as_ref(),
            overlay_chunk_buffer.buffer.as_ref(),
            view_uniforms.uniforms.binding(), // Dynamic uniform buffer with offsets
        )
        else {
            // Buffers not ready yet, skip rendering this frame
            // 缓冲区尚未准备好，跳过本帧渲染
            return Ok(());
        };

        // Get GPU texture views for fog texture arrays with fallback support
        // 获取雾效纹理数组的GPU纹理视图，支持回退
        let visibility_texture_view = images
            .get(&visibility_texture.0) // Try to get real-time visibility texture
            .map(|img| &img.texture_view)
            .unwrap_or(&fallback_image.d2.texture_view); // Fallback if not available

        let fog_texture_view = images
            .get(&fog_texture.0) // Try to get persistent fog texture
            .map(|img| &img.texture_view)
            .unwrap_or(&fallback_image.d2.texture_view); // Fallback if not available

        let snapshot_texture_view = images
            .get(&snapshot_texture.0) // Try to get snapshot texture array
            .map(|img| &img.texture_view)
            .unwrap_or(&fallback_image.d2.texture_view); // Fallback if not available

        // Create view-specific bind group with all required GPU resources
        // 创建包含所有必需GPU资源的视图特定绑定组
        let bind_group = render_context.render_device().create_bind_group(
            "fog_overlay_bind_group",
            &overlay_pipeline.layout,
            &BindGroupEntries::sequential((
                view_uniform_binding,            // 0: Camera view uniforms (with dynamic offset)
                &overlay_pipeline.sampler,       // 1: Linear filtering sampler for textures
                visibility_texture_view,         // 2: Real-time visibility texture array
                fog_texture_view,                // 3: Persistent fog exploration texture array
                snapshot_texture_view,           // 4: Captured entity snapshot texture array
                uniform_buf.as_entire_binding(), // 5: Fog settings uniform buffer
                mapping_buf.as_entire_binding(), // 6: Chunk coordinate mapping storage buffer
            )),
        );

        // Begin fullscreen render pass targeting the current view
        // 开始针对当前视图的全屏渲染通道
        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("fog_overlay_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: view_target.main_texture_view(), // Render to main view target
                resolve_target: None,                  // No MSAA resolve needed
                ops: Operations {
                    load: LoadOp::Load,    // Load existing scene content
                    store: StoreOp::Store, // Store final composited result
                },
            })],
            depth_stencil_attachment: None, // No depth testing for fullscreen overlay
            timestamp_writes: None,         // No GPU timing queries
            occlusion_query_set: None,      // No occlusion queries needed
        });

        // Configure render pass with pipeline and resources
        // 使用管线和资源配置渲染通道
        render_pass.set_render_pipeline(pipeline);

        // Bind all GPU resources with view-specific dynamic offset
        // 使用视图特定的动态偏移绑定所有GPU资源
        render_pass.set_bind_group(0, &bind_group, &[view_uniform_offset.offset]);

        // Execute fullscreen triangle rendering for fog overlay compositing
        // 执行全屏三角形渲染以进行雾效覆盖合成
        // Uses 3 vertices (fullscreen triangle) with 1 instance for optimal GPU utilization
        // 使用3个顶点（全屏三角形）和1个实例以实现最佳GPU利用率
        render_pass.draw(0..3, 0..1);

        Ok(()) // Rendering completed successfully
    }
}
