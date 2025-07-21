use crate::prelude::*;
use bevy::asset::Handle;
use bevy::prelude::Image;
use bevy::prelude::Resource;
use bevy::reflect::Reflect;

/// Resource handle for the GPU texture array storing real-time visibility data.
/// 存储实时可见性数据的GPU纹理数组资源句柄
///
/// This resource manages the handle to a 3D texture array that stores the current
/// visibility state for all active fog of war chunks. Each layer in the array
/// corresponds to one chunk's visibility data, typically in R8Unorm format.
///
/// # Texture Array Structure
/// - **Format**: R8Unorm (single-channel, 8-bit normalized)
/// - **Dimensions**: texture_resolution_per_chunk.x × texture_resolution_per_chunk.y × max_layers
/// - **Layer Count**: Configurable via FogMapSettings.array_layers_capacity
/// - **Usage**: GPU compute shaders read/write visibility data per chunk
///
/// # Performance Characteristics
/// - **Memory**: ~256KB per layer for 256×256 R8 texture (typical configuration)
/// - **GPU Access**: Optimized for frequent read/write in compute shaders
/// - **Allocation**: Single large array reduces GPU memory fragmentation
/// - **Time Complexity**: O(1) layer access, O(texture_size) for compute operations
///
/// # Integration Points
/// - **Compute Shaders**: Primary data source for fog visibility calculations
/// - **TextureArrayManager**: Manages layer allocation within this array
/// - **Render Pipeline**: Used in fog overlay shader for final rendering
/// - **Memory Management**: Can be transferred to/from CPU via data transfer system
///
/// # Usage Example
/// ```rust,no_run
/// // Access in system
/// fn system(visibility_array: Res<VisibilityTextureArray>) {
///     let texture_handle = &visibility_array.handle;
///     // Use handle with GPU rendering systems
/// }
/// ```
///
/// # Asset Lifecycle
/// - **Creation**: Initialized during fog system setup
/// - **Lifetime**: Persistent throughout application lifetime
/// - **Updates**: Content modified by compute shaders, handle remains constant
/// - **Cleanup**: Automatically managed by Bevy's asset system
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct VisibilityTextureArray {
    /// Handle to the 3D texture array asset storing visibility data.
    /// 存储可见性数据的3D纹理数组资源句柄
    ///
    /// This handle references a GPU texture array where each layer contains
    /// the real-time visibility data for one fog of war chunk. The texture
    /// is actively modified by compute shaders during fog calculations.
    pub handle: Handle<Image>,
}

/// Resource handle for the GPU texture array storing persistent fog exploration data.
/// 存储持久性雾效探索数据的GPU纹理数组资源句柄
///
/// This resource manages the handle to a 3D texture array that stores the cumulative
/// exploration state for all fog of war chunks. Unlike visibility data which changes
/// in real-time, fog data represents permanent exploration history.
///
/// # Texture Array Structure
/// - **Format**: R8Unorm (single-channel, 8-bit normalized)
/// - **Dimensions**: texture_resolution_per_chunk.x × texture_resolution_per_chunk.y × max_layers
/// - **Layer Count**: Matches VisibilityTextureArray layer capacity
/// - **Data Semantics**: 0.0 = unexplored, 1.0 = fully explored, intermediate = partially explored
///
/// # Data Persistence
/// Unlike visibility data which resets when vision sources move away, fog data
/// accumulates over time and represents the player's exploration history:
/// - **Write-Only Updates**: Only increases exploration, never decreases
/// - **Persistence**: Survives game sessions when saved/loaded
/// - **Memory Transfer**: Can be moved between CPU/GPU for persistence operations
/// - **Accumulation Logic**: max(current_fog, new_visibility) in compute shaders
///
/// # Performance Characteristics
/// - **Memory**: ~256KB per layer for 256×256 R8 texture (typical configuration)
/// - **Update Frequency**: Less frequent than visibility (only when exploring new areas)
/// - **GPU Bandwidth**: Write operations during exploration, read during rendering
/// - **Time Complexity**: O(1) layer access, O(texture_size) for compute operations
///
/// # Integration Points
/// - **Compute Shaders**: Updated when visibility reveals unexplored areas
/// - **Fog Overlay Shader**: Primary data source for rendering fog effects
/// - **Persistence System**: Saved/loaded for game state preservation
/// - **TextureArrayManager**: Coordinates layer allocation with visibility array
///
/// # Usage Example
/// ```rust,no_run
/// // Access in rendering system
/// fn fog_rendering_system(fog_array: Res<FogTextureArray>) {
///     let fog_texture_handle = &fog_array.handle;
///     // Bind to GPU for fog overlay rendering
/// }
/// ```
///
/// # Data Flow
/// ```text
/// VisionSource → VisibilityTexture → ComputeShader → FogTexture → FogOverlay → Screen
/// ```
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct FogTextureArray {
    /// Handle to the 3D texture array asset storing persistent fog exploration data.
    /// 存储持久性雾效探索数据的3D纹理数组资源句柄
    ///
    /// This handle references a GPU texture array where each layer contains
    /// the cumulative exploration history for one fog of war chunk. Values
    /// represent how much of each area has been explored over time.
    pub handle: Handle<Image>,
}

/// Resource handle for the GPU texture array storing snapshot images of explored areas.
/// 存储已探索区域快照图像的GPU纹理数组资源句柄
///
/// This resource manages the handle to a 3D texture array that stores visual snapshots
/// of previously explored areas. These snapshots capture the appearance of entities
/// and terrain when they were first discovered, providing visual continuity in fog of war.
///
/// # Texture Array Structure
/// - **Format**: RGBA8 (4-channel, 8-bit per channel) for full color capture
/// - **Dimensions**: texture_resolution_per_chunk.x × texture_resolution_per_chunk.y × max_layers
/// - **Layer Count**: Matches other texture arrays for coordinated chunk management
/// - **Content**: Rendered images of entities marked with Capturable component
///
/// # Snapshot Capture Process
/// Snapshots are captured when chunks are first explored:
/// 1. **Camera Positioning**: Snapshot camera positioned over chunk bounds
/// 2. **Entity Rendering**: Capturable entities rendered to temporary texture
/// 3. **Texture Copy**: Temporary texture copied to specific array layer
/// 4. **Visual Preservation**: Image stored for future rendering when chunk is obscured
///
/// # Performance Characteristics
/// - **Memory**: ~1MB per layer for 256×256 RGBA8 texture (4× larger than fog data)
/// - **Update Frequency**: Very low - only when chunks are first explored
/// - **GPU Usage**: Render-to-texture operations during snapshot capture
/// - **Time Complexity**: O(1) layer access, O(entities) for snapshot rendering
///
/// # Visual Continuity
/// Provides seamless visual experience in fog of war:
/// - **Explored Areas**: Show last known state of entities and terrain
/// - **Dynamic Content**: Captures moving entities at time of discovery
/// - **Layer Coordination**: Synchronized with fog texture array layers
/// - **Render Integration**: Blended with fog overlay in final composition
///
/// # Integration Points
/// - **Snapshot Camera**: Renders entities to capture visual state
/// - **Fog Overlay Shader**: Composites snapshots with fog effects
/// - **TextureArrayManager**: Coordinates layer allocation across texture types
/// - **Capturable System**: Determines which entities appear in snapshots
///
/// # Usage Example
/// ```rust,no_run
/// // Access in fog overlay system
/// fn fog_overlay_system(snapshot_array: Res<SnapshotTextureArray>) {
///     let snapshot_handle = &snapshot_array.handle;
///     // Bind to GPU for fog overlay composition
/// }
/// ```
///
/// # Memory Considerations
/// Snapshot textures require significantly more memory than fog data due to RGBA format.
/// Consider texture resolution and layer count when configuring for memory-constrained platforms.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct SnapshotTextureArray {
    /// Handle to the 3D texture array asset storing visual snapshots.
    /// 存储视觉快照的3D纹理数组资源句柄
    ///
    /// This handle references a GPU texture array where each layer contains
    /// a rendered snapshot of entities as they appeared when first explored.
    /// Used for visual continuity in previously explored but currently obscured areas.
    pub handle: Handle<Image>,
}

/// Resource handle for the temporary texture used during snapshot capture operations.
/// 快照捕获操作期间使用的临时纹理资源句柄
///
/// This resource manages the handle to a temporary 2D texture that serves as an
/// intermediate render target during snapshot capture. The snapshot camera renders
/// to this texture before the final copy to the snapshot texture array.
///
/// # Texture Structure
/// - **Format**: RGBA8 (matches snapshot array format for direct copying)
/// - **Dimensions**: texture_resolution_per_chunk.x × texture_resolution_per_chunk.y
/// - **Type**: 2D render target (not array layer)
/// - **Usage**: RENDER_ATTACHMENT | TEXTURE_BINDING | COPY_SRC | COPY_DST
///
/// # Capture Workflow
/// The temporary texture enables efficient snapshot capture:
/// 1. **Render Target**: Snapshot camera renders Capturable entities to this texture
/// 2. **Frame Buffer**: Serves as frame buffer for off-screen rendering
/// 3. **Source Buffer**: Acts as source for texture copy to final array layer
/// 4. **Reuse**: Single texture reused for all snapshot operations
///
/// # Performance Benefits
/// - **Memory Efficiency**: Single temp texture vs. multiple render targets
/// - **GPU Optimization**: Direct texture-to-texture copy operations
/// - **Resource Reuse**: Eliminates allocation/deallocation per snapshot
/// - **Pipeline Integration**: Seamless integration with render graph
///
/// # Technical Details
/// - **Lifetime**: Created once during system initialization
/// - **State**: Contents overwritten for each new snapshot
/// - **Synchronization**: Render graph ensures proper timing for copy operations
/// - **Format Matching**: Identical format to destination array for efficient copies
///
/// # Integration Points
/// - **Snapshot Camera**: Uses as render target for entity capture
/// - **SnapshotNode**: Copies from this texture to final array layer
/// - **Render Graph**: Manages texture lifecycle and synchronization
/// - **Asset System**: Handles GPU resource management
///
/// # Usage Pattern
/// ```text
/// Setup: Create temp texture
/// Per Snapshot: Camera → Temp Texture → Array Layer
/// Cleanup: Automatic via asset system
/// ```
///
/// # Memory Characteristics
/// - **Allocation**: ~1MB for 256×256 RGBA8 texture (typical configuration)
/// - **Persistence**: Persistent but content is transient
/// - **GPU Memory**: Single allocation, reused across all snapshot operations
/// - **Efficiency**: Avoids repeated allocation/deallocation overhead
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct SnapshotTempTexture {
    /// Handle to the temporary 2D texture asset used for snapshot capture.
    /// 用于快照捕获的临时2D纹理资源句柄
    ///
    /// This handle references a GPU texture that serves as an intermediate
    /// render target during snapshot capture operations. The snapshot camera
    /// renders to this texture before copying to the final snapshot array layer.
    pub handle: Handle<Image>,
}
