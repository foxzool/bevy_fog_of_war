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
use bevy_asset::DirectAssetAccessExt;
use bevy_core_pipeline::FullscreenShader;
use bevy_ecs::prelude::*;
use bevy_render::{
    render_asset::RenderAssets,
    render_resource::binding_types::{
        sampler, storage_buffer_read_only, texture_2d_array, uniform_buffer,
    },
    render_resource::*,
    renderer::{RenderContext, RenderDevice},
    texture::{FallbackImage, GpuImage},
    view::{ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
};

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
    layout: BindGroupLayoutDescriptor,

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

        // Create bind group layout descriptor (Bevy 0.18: BindGroupLayoutDescriptor not BindGroupLayout)
        // 创建绑定组布局描述符 (Bevy 0.18: BindGroupLayoutDescriptor 而非 BindGroupLayout)
        let layout = BindGroupLayoutDescriptor::new(
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
            mipmap_filter: MipmapFilterMode::Linear, // Linear mip mapping for level transitions
            address_mode_u: AddressMode::ClampToEdge, // Clamp U axis to prevent sampling artifacts
            address_mode_v: AddressMode::ClampToEdge, // Clamp V axis to prevent sampling artifacts
            address_mode_w: AddressMode::ClampToEdge, // Clamp W axis for texture array layers
            ..Default::default()
        });

        // Load fog overlay fragment shader asset
        // 加载雾效覆盖片段着色器资源
        let shader = world.load_asset(SHADER_ASSET_PATH);

        // Get the fullscreen vertex shader handle from the FullscreenShader resource
        // 从 FullscreenShader 资源获取全屏顶点着色器句柄
        let fullscreen_shader = world.resource::<FullscreenShader>().shader().clone();

        // Queue render pipeline for compilation with complete configuration
        // 排队渲染管线以进行完整配置的编译
        let pipeline_id =
            world
                .resource_mut::<PipelineCache>()
                .queue_render_pipeline(RenderPipelineDescriptor {
                    label: Some("fog_overlay_pipeline_init".into()), // Pipeline identifier for debugging
                    layout: vec![layout.clone()], // Use the bind group layout created above
                    vertex: VertexState {
                        shader: fullscreen_shader, // Bevy's built-in fullscreen vertex shader
                        shader_defs: vec![],       // No shader preprocessor definitions
                        entry_point: None,         // Use default entry point from shader
                        buffers: vec![],           // No vertex buffers (fullscreen triangle)
                    },
                    fragment: Some(FragmentState {
                        shader,              // Custom fog overlay fragment shader
                        shader_defs: vec![], // No shader preprocessor definitions
                        entry_point: None,   // Use default entry point from shader
                        targets: vec![Some(ColorTargetState {
                            format: TextureFormat::Rgba8UnormSrgb, // Standard RGBA format for overlay
                            blend: Some(BlendState::ALPHA_BLENDING), // Standard alpha blending for transparency
                            write_mask: ColorWrites::ALL,            // Write to all RGBA channels
                        })],
                    }),
                    primitive: PrimitiveState::default(), // Default primitive settings (triangle list)
                    depth_stencil: None,                  // No depth testing for fullscreen overlay
                    multisample: MultisampleState::default(), // Default multisampling settings
                    immediate_size: 0,                    // No immediate mode data
                    zero_initialize_workgroup_memory: false,
                });

        // Return configured pipeline with all components
        // 返回包含所有组件的配置管线
        FogOverlayPipeline {
            layout,      // Bind group layout descriptor for resource binding
            sampler,     // Texture sampler for filtering
            pipeline_id, // Cached pipeline ID for runtime retrieval
        }
    }
}

/// Executes fog overlay rendering for all non-snapshot camera views.
/// 为所有非快照相机视图执行雾效覆盖渲染
///
/// This system is called by Bevy's render schedule for each frame.
/// It performs fullscreen fog compositing by sampling fog textures and blending
/// them over the main scene for each active camera view.
///
/// # Execution Flow
/// 1. **Pipeline Validation**: Ensure render pipeline is compiled and ready
/// 2. **Buffer Validation**: Verify all uniform and storage buffers are prepared
/// 3. **Texture Access**: Get texture views for fog, visibility, and snapshot arrays
/// 4. **Bind Group Creation**: Create shared bind group with all resources
/// 5. **Render Pass**: Execute fullscreen triangle rendering for each view
///
/// # Performance Characteristics
/// - **Resolution Dependent**: O(screen_width × screen_height) fragment operations per view
/// - **Texture Bandwidth**: 3-6 texture samples per pixel for fog data
/// - **Fill Rate Bound**: Limited by GPU fragment processing and texture cache
/// - **Memory Access**: Coalesced texture array access for cache efficiency
///
/// # Error Handling
/// Returns silently in all cases to maintain render stability:
/// - **Missing Pipeline**: Waits for shader compilation without blocking
/// - **Missing Buffers**: Skips rendering when GPU buffers not ready
/// - **Missing Textures**: Uses fallback textures for graceful degradation
pub fn fog_overlay_system(
    mut render_context: RenderContext,
    views: Query<(Entity, &ViewTarget, &ViewUniformOffset), Without<SnapshotCamera>>,
    overlay_pipeline: Res<FogOverlayPipeline>,
    pipeline_cache: Res<PipelineCache>,
    fog_uniforms: Res<FogUniforms>,
    overlay_chunk_buffer: Res<OverlayChunkMappingBuffer>,
    visibility_texture: Res<RenderVisibilityTexture>,
    fog_texture: Res<RenderFogTexture>,
    snapshot_texture: Res<RenderSnapshotTexture>,
    images: Res<RenderAssets<GpuImage>>,
    fallback_image: Res<FallbackImage>,
    view_uniforms: Res<ViewUniforms>,
) {
    // Retrieve compiled render pipeline from cache
    let Some(pipeline) = pipeline_cache.get_render_pipeline(overlay_pipeline.pipeline_id)
    else {
        // Pipeline not compiled yet, skip this frame gracefully
        return;
    };

    // Validate that all required GPU buffers are prepared and ready
    let (
        Some(uniform_buf),
        Some(mapping_buf),
        Some(view_uniform_binding),
    ) = (
        fog_uniforms.buffer.as_ref(),
        overlay_chunk_buffer.buffer.as_ref(),
        view_uniforms.uniforms.binding(),
    )
    else {
        // Buffers not ready yet, skip rendering this frame
        return;
    };

    // Get GPU texture views for fog texture arrays with fallback support
    let visibility_texture_view = images
        .get(&visibility_texture.0)
        .map(|img| &img.texture_view)
        .unwrap_or(&fallback_image.d2.texture_view);

    let fog_texture_view = images
        .get(&fog_texture.0)
        .map(|img| &img.texture_view)
        .unwrap_or(&fallback_image.d2.texture_view);

    let snapshot_texture_view = images
        .get(&snapshot_texture.0)
        .map(|img| &img.texture_view)
        .unwrap_or(&fallback_image.d2.texture_view);

    // Resolve bind group layout from pipeline cache
    let overlay_layout = pipeline_cache.get_bind_group_layout(&overlay_pipeline.layout);

    // Create shared bind group for all views
    let bind_group = render_context.render_device().create_bind_group(
        "fog_overlay_bind_group",
        &overlay_layout,
        &BindGroupEntries::sequential((
            view_uniform_binding,
            &overlay_pipeline.sampler,
            visibility_texture_view,
            fog_texture_view,
            snapshot_texture_view,
            uniform_buf.as_entire_binding(),
            mapping_buf.as_entire_binding(),
        )),
    );

    // Iterate over all non-snapshot views and render fog overlay
    for (_view_entity, view_target, view_uniform_offset) in &views {
        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("fog_overlay_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: view_target.main_texture_view(),
                resolve_target: None,
                depth_slice: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        render_pass.set_render_pipeline(pipeline);
        render_pass.set_bind_group(0, &bind_group, &[view_uniform_offset.offset]);
        render_pass.draw(0..3, 0..1);
    }
}
