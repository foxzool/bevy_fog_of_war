//! # Bevy Fog of War Plugin

// Allow collapsible_if for stable Rust compatibility
#![allow(clippy::collapsible_if)]
//!
//! A comprehensive 2D fog of war implementation for the Bevy game engine.
//! This plugin provides chunk-based fog processing with GPU compute shaders,
//! multiple vision shapes, explored area tracking, and persistence functionality.
//!
//! ## Architecture
//!
//! The plugin uses a chunk-based system where the world is divided into configurable
//! chunks (default 256x256 units). Each chunk can be in different visibility states:
//! - **Unexplored**: Not yet discovered by any vision source
//! - **Explored**: Previously visible but currently out of sight
//! - **Visible**: Currently within range of an active vision source
//!
//! ## Memory Management
//!
//! Chunks are dynamically managed between CPU and GPU memory based on visibility
//! and camera view. This allows for efficient handling of large worlds while
//! maintaining good performance.
//!
//! ## Core Systems
//!
//! The plugin orchestrates several key systems in a specific order:
//! 1. **UpdateChunkState**: Updates chunk visibility based on vision sources and camera
//! 2. **ManageEntities**: Creates and manages chunk entities
//! 3. **Persistence**: Handles save/load operations
//! 4. **PrepareTransfers**: Manages CPU/GPU memory transfers
//!
//! ## Usage Example
//!
//! ```rust
//! use bevy::prelude::*;
//! use bevy_fog_of_war::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(FogOfWarPlugin)
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands) {
//!     // Add camera with fog of war support
//!     commands.spawn((Camera2d, FogOfWarCamera));
//!
//!     // Add entities with vision
//!     commands.spawn((
//!         Transform::from_xyz(0.0, 0.0, 0.0),
//!         VisionSource::circle(200.0)
//!     ));
//! }
//! ```

use self::prelude::*;
use crate::persistence::FogOfWarPersistencePlugin;
use crate::render::FogOfWarRenderPlugin;
use bevy_asset::{Assets, RenderAssetUsages};
use bevy_image::{Image, ImageSampler, ImageSamplerDescriptor};
use bevy_math::{IVec2, Rect, Vec2};
use bevy_platform::collections::HashSet;
use bevy_render::camera::RenderTarget;
use bevy_render::extract_component::ExtractComponentPlugin;
use bevy_render::extract_resource::ExtractResourcePlugin;
use bevy_render::render_resource::{Extent3d, TextureDimension, TextureUsages};
use bevy_time::Time;
use bevy_transform::components::GlobalTransform;

mod components;
mod data_transfer;
mod managers;
pub mod persistence;
pub mod persistence_utils;
pub mod prelude;
mod render;
mod settings;
mod snapshot;
mod texture_handles;

/// Event to request a snapshot for a specific chunk.
/// 请求为特定区块生成快照的事件。
///
/// This event is used to trigger snapshot capture for chunks that have changed
/// visibility state, allowing the system to preserve the visual state of explored
/// areas when they become no longer visible.
///
/// # Performance Considerations
/// - **Complexity**: O(1) - Single chunk snapshot
/// - **Frequency**: Triggered when chunks transition visibility states
/// - **Cost**: Moderate - requires render-to-texture operation
///
/// # Usage
/// The system automatically sends this event when:
/// - A chunk becomes explored for the first time
/// - A chunk transitions from explored to visible
/// - A chunk re-enters visibility after being out of sight
#[derive(Event, Debug, Clone, Copy)]
pub struct RequestChunkSnapshot(pub IVec2);

/// System sets that define the execution order of fog of war systems.
/// 定义雾效系统执行顺序的系统集。
///
/// These system sets are configured to run in a specific order using `.chain()`,
/// ensuring that data dependencies are respected and the fog of war state
/// remains consistent throughout each frame.
///
/// # Execution Order
/// The systems run in this order within each frame:
/// 1. **UpdateChunkState** - Processes vision sources and updates visibility
/// 2. **ManageEntities** - Creates/destroys chunk entities as needed
/// 3. **Persistence** - Handles save/load operations
/// 4. **PrepareTransfers** - Queues memory transfers between CPU and GPU
///
/// # Performance Considerations
/// - **Total Complexity**: O(V×C + E) where V=vision sources, C=chunks in range, E=entities
/// - **Frame Cost**: Varies based on vision source movement and chunk transitions
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum FogSystems {
    /// Update chunk states based on vision and camera position.
    /// 更新区块状态 (基于视野和相机)
    ///
    /// This system set includes:
    /// - `clear_per_frame_caches` - Clears frame-specific data
    /// - `update_chunk_visibility` - Processes vision sources
    /// - `update_camera_view_chunks` - Updates camera view area
    /// - `update_chunk_component_state` - Syncs cache to components
    ///
    /// **Complexity**: O(V×C) where V=vision sources, C=chunks in vision range
    UpdateChunkState,

    /// Manage chunk entities (creation, activation, deactivation).
    /// 管理区块实体 (创建, 激活)
    ///
    /// This system set includes:
    /// - `manage_chunk_entities` - Creates entities for new chunks
    ///
    /// **Complexity**: O(E) where E=number of required chunk entities
    ManageEntities,

    /// Handle persistence operations (save/load).
    /// 处理持久化操作 (保存/加载)
    ///
    /// This system set includes:
    /// - `save_fog_of_war_system` - Processes save requests
    /// - `handle_gpu_data_ready_system` - Handles GPU data completion
    /// - `load_fog_of_war_system` - Processes load requests
    ///
    /// **Complexity**: O(S) where S=number of chunks being saved/loaded
    Persistence,

    /// Handle CPU <-> GPU memory transfer logic.
    /// 处理 CPU <-> GPU 内存传输逻辑
    ///
    /// This system set includes:
    /// - `manage_chunk_texture_transfer` - Orchestrates memory transfers
    ///
    /// **Complexity**: O(T) where T=number of chunks being transferred
    PrepareTransfers,
}

/// The main fog of war plugin for Bevy applications.
/// 主要的Bevy应用程序雾效插件。
///
/// This plugin orchestrates all fog of war functionality including:
/// - Chunk-based visibility processing
/// - GPU compute shader rendering
/// - Memory management between CPU and GPU
/// - Persistence and serialization
/// - Event handling and state management
///
/// # Dependencies
/// This plugin depends on and integrates with:
/// - `FogOfWarRenderPlugin` - Handles GPU rendering and compute shaders
/// - `SnapshotPlugin` - Manages explored area snapshots
/// - `FogOfWarPersistencePlugin` - Provides save/load functionality
///
/// # Resource Initialization
/// The plugin automatically initializes all required resources:
/// - `FogMapSettings` - Configuration settings
/// - `ChunkEntityManager` - Manages chunk entities
/// - `ChunkStateCache` - Caches chunk visibility states
/// - Texture arrays for fog, visibility, and snapshots
/// - Memory transfer request queues
///
/// # Performance Impact
/// - **Startup Cost**: O(T) where T=texture array size
/// - **Per-Frame Cost**: O(V×C) where V=vision sources, C=visible chunks
/// - **Memory Usage**: Scales with number of active chunks and texture resolution
///
/// # Example Usage
/// ```rust
/// use bevy::prelude::*;
/// use bevy_fog_of_war::FogOfWarPlugin;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(FogOfWarPlugin)
///     .run();
/// ```
pub struct FogOfWarPlugin;

impl Plugin for FogOfWarPlugin {
    /// Builds the fog of war plugin, registering all systems, resources, and events.
    /// 构建雾效插件，注册所有系统、资源和事件。
    ///
    /// This method sets up the complete fog of war infrastructure:
    /// 1. Registers reflection types for editor support
    /// 2. Initializes core resources and caches
    /// 3. Adds events for system communication
    /// 4. Configures extraction plugins for render world
    /// 5. Sets up system execution order with dependencies
    /// 6. Adds child plugins for rendering, snapshots, and persistence
    ///
    /// # Complexity
    /// **Time**: O(1) - Setup cost is constant
    /// **Space**: O(T) where T=maximum texture array layers
    fn build(&self, app: &mut App) {
        app.register_type::<VisionSource>()
            .register_type::<FogChunk>()
            .register_type::<Capturable>()
            .register_type::<ForceSnapshotCapturables>()
            .register_type::<ChunkVisibility>()
            .register_type::<ChunkMemoryLocation>()
            .register_type::<ChunkState>()
            // .register_type::<FogMapSettings>()
            .register_type::<FogTextureArray>()
            .register_type::<SnapshotTextureArray>()
            .register_type::<ChunkEntityManager>()
            .register_type::<ChunkStateCache>()
            .register_type::<TextureArrayManager>()
            .register_type::<FogChunkImage>()
            .register_type::<GpuToCpuCopyRequests>()
            .register_type::<CpuToGpuCopyRequests>()
            .register_type::<MainWorldSnapshotRequestQueue>();

        app.init_resource::<FogMapSettings>()
            .init_resource::<ChunkEntityManager>()
            .init_resource::<ChunkStateCache>()
            .init_resource::<GpuToCpuCopyRequests>()
            .init_resource::<CpuToGpuCopyRequests>()
            .init_resource::<MainWorldSnapshotRequestQueue>()
            .init_resource::<FogResetSync>();

        app.add_event::<ChunkGpuDataReady>()
            .add_event::<ChunkCpuDataUploaded>()
            .add_event::<RequestChunkSnapshot>() // Added event for remaking snapshots / 添加用于重制快照的事件
            .add_event::<ResetFogOfWar>() // Added event for resetting fog of war / 添加用于重置雾效的事件
            .add_event::<FogResetSuccess>() // Added event for successful reset / 添加用于成功重置的事件
            .add_event::<FogResetFailed>(); // Added event for failed reset / 添加用于失败重置的事件

        app.add_plugins(ExtractResourcePlugin::<GpuToCpuCopyRequests>::default())
            .add_plugins(ExtractResourcePlugin::<CpuToGpuCopyRequests>::default())
            .add_plugins(ExtractResourcePlugin::<FogResetSync>::default())
            .add_plugins(ExtractComponentPlugin::<SnapshotCamera>::default());

        app.configure_sets(
            Update,
            (
                FogSystems::UpdateChunkState,
                FogSystems::ManageEntities,
                FogSystems::Persistence,
                FogSystems::PrepareTransfers,
            )
                .chain(), // Ensure they run in this order / 确保它们按此顺序运行
        );

        app.add_systems(Startup, setup_fog_resources);

        app.add_systems(
            Update,
            (
                clear_per_frame_caches,
                update_chunk_visibility,
                update_camera_view_chunks,
                update_chunk_component_state,
            )
                .chain()
                .in_set(FogSystems::UpdateChunkState),
        );

        app.add_systems(
            Update,
            (manage_chunk_entities).in_set(FogSystems::ManageEntities),
        );

        app.add_systems(
            Update,
            manage_chunk_texture_transfer.in_set(FogSystems::PrepareTransfers),
        );

        app.add_systems(Update, (reset_fog_of_war_system, monitor_reset_sync_system));

        app.add_plugins(FogOfWarRenderPlugin);
        app.add_plugins(SnapshotPlugin);
        app.add_plugins(FogOfWarPersistencePlugin);
    }
}

/// Initializes core fog of war texture resources and managers.
/// 初始化核心雾效纹理资源和管理器。
///
/// This function sets up the fundamental texture arrays required for fog of war:
/// - **Fog Texture Array**: R8Unorm format for fog visibility (0=visible, 1=unexplored)
/// - **Visibility Texture Array**: R8Unorm format for current frame visibility
/// - **Snapshot Texture Array**: Rgba8UnormSrgb format for explored area snapshots
///
/// Each texture array supports `MAX_LAYERS` concurrent chunks in GPU memory.
/// Textures are configured with appropriate usage flags for compute shaders,
/// sampling, and CPU/GPU transfers.
///
/// # Safety
/// Uses checked arithmetic to prevent integer overflow when calculating texture sizes.
/// Will panic if texture dimensions would cause overflow, indicating invalid settings.
///
/// # Performance Considerations
/// - **Complexity**: O(T×R²) where T=MAX_LAYERS, R=resolution per chunk
/// - **Memory Usage**: ~(R² × T × 6) bytes for all texture arrays combined
/// - **GPU Memory**: All textures are created in GPU memory with render asset usage
///
/// # Panics
/// - If texture size calculations overflow (indicates invalid configuration)
/// - If required texture formats are not supported by the GPU
///
/// # Dependencies
/// - Requires `FogMapSettings` resource to be initialized
/// - Modifies `Assets<Image>` to add texture resources
/// - Creates `TextureArrayManager` with `MAX_LAYERS` capacity
fn setup_fog_resources(
    mut commands: Commands,
    settings: Res<FogMapSettings>,
    mut images: ResMut<Assets<Image>>,
) {
    // --- Create Texture Arrays ---
    // --- 创建 Texture Arrays ---

    let fog_texture_size = Extent3d {
        width: settings.texture_resolution_per_chunk.x,
        height: settings.texture_resolution_per_chunk.y,
        depth_or_array_layers: MAX_LAYERS,
    };
    let snapshot_texture_size = fog_texture_size;
    let visibility_texture_size = fog_texture_size;

    // Fog Texture: R8Unorm (0=visible, 1=unexplored)
    // 雾效纹理: R8Unorm (0=可见, 1=未探索)
    // 安全的纹理大小计算，防止整数溢出
    // Safe texture size calculation to prevent integer overflow
    let fog_data_size = (fog_texture_size.width as u64)
        .checked_mul(fog_texture_size.height as u64)
        .and_then(|v| v.checked_mul(fog_texture_size.depth_or_array_layers as u64))
        .and_then(|v| usize::try_from(v).ok())
        .expect("Fog texture size too large, would cause integer overflow");

    let fog_initial_data = vec![0u8; fog_data_size];
    let mut fog_image = Image::new(
        fog_texture_size,
        TextureDimension::D2,
        fog_initial_data,
        settings.fog_texture_format,
        RenderAssetUsages::RENDER_WORLD,
    );
    fog_image.texture_descriptor.usage = TextureUsages::STORAGE_BINDING // For compute shader write / 用于 compute shader 写入
        | TextureUsages::TEXTURE_BINDING // For sampling in overlay shader / 用于在覆盖 shader 中采样
        | TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
        | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输
    fog_image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());

    // 安全的可见性纹理大小计算，防止整数溢出
    // Safe visibility texture size calculation to prevent integer overflow
    let visibility_data_size = (visibility_texture_size.width as u64)
        .checked_mul(visibility_texture_size.height as u64)
        .and_then(|v| v.checked_mul(visibility_texture_size.depth_or_array_layers as u64))
        .and_then(|v| usize::try_from(v).ok())
        .expect("Visibility texture size too large, would cause integer overflow");

    let visibility_initial_data = vec![0u8; visibility_data_size];
    let mut visibility_image = Image::new(
        visibility_texture_size,
        TextureDimension::D2,
        visibility_initial_data,
        settings.fog_texture_format, // same format as fog texture
        RenderAssetUsages::default(),
    );
    visibility_image.texture_descriptor.usage = TextureUsages::STORAGE_BINDING // For compute shader write / 用于 compute shader 写入
        | TextureUsages::TEXTURE_BINDING // For sampling in overlay shader / 用于在覆盖 shader 中采样
        | TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
        | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输
    visibility_image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());

    // Snapshot Texture: Rgba8UnormSrgb (Stores last visible scene)
    // 快照纹理: Rgba8UnormSrgb (存储最后可见的场景)
    // 安全的快照纹理大小计算（包含4字节RGBA），防止整数溢出
    // Safe snapshot texture size calculation (including 4-byte RGBA) to prevent integer overflow
    let snapshot_data_size = (snapshot_texture_size.width as u64)
        .checked_mul(snapshot_texture_size.height as u64)
        .and_then(|v| v.checked_mul(snapshot_texture_size.depth_or_array_layers as u64))
        .and_then(|v| v.checked_mul(4u64)) // 4 bytes per pixel for RGBA / RGBA 每像素 4 字节
        .and_then(|v| usize::try_from(v).ok())
        .expect("Snapshot texture size too large, would cause integer overflow");

    let snapshot_initial_data = vec![0u8; snapshot_data_size];
    let mut snapshot_image = Image::new(
        snapshot_texture_size,
        TextureDimension::D2,
        snapshot_initial_data,
        settings.snapshot_texture_format,
        RenderAssetUsages::default(),
    );
    snapshot_image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT // To render snapshots into / 用于渲染快照
        | TextureUsages::TEXTURE_BINDING // For sampling in overlay shader / 用于在覆盖 shader 中采样
        | TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
        | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输

    let fog_handle = images.add(fog_image);
    let visibility_handle = images.add(visibility_image);
    let snapshot_handle = images.add(snapshot_image);

    // Insert resources
    // 插入资源
    commands.insert_resource(FogTextureArray { handle: fog_handle });
    commands.insert_resource(VisibilityTextureArray {
        handle: visibility_handle,
    });
    commands.insert_resource(SnapshotTextureArray {
        handle: snapshot_handle.clone(),
    });
    commands.insert_resource(TextureArrayManager::new(MAX_LAYERS));

    info!("Fog of War resources initialized");
}

/// Clears frame-specific caches that need to be rebuilt each frame.
/// 清除每帧重建的缓存。
///
/// This function resets the visibility and camera view caches at the start of each frame,
/// ensuring that they accurately reflect the current frame's state. The explored chunks
/// cache is preserved as it represents persistent discovery state.
///
/// # Cleared Caches
/// - `visible_chunks`: Chunks currently within vision source range
/// - `camera_view_chunks`: Chunks within the camera's viewport
///
/// # Preserved Caches
/// - `explored_chunks`: Permanently discovered chunks (persistent across frames)
/// - `gpu_resident_chunks`: Chunks currently in GPU memory (managed separately)
///
/// # Performance
/// - **Complexity**: O(1) - Hash set clear operations
/// - **Memory**: Frees temporary allocations from previous frame
/// - **Frequency**: Called once per frame before visibility updates
///
/// # System Dependencies
/// Must run before:
/// - `update_chunk_visibility`
/// - `update_camera_view_chunks`
/// - `update_chunk_component_state`
fn clear_per_frame_caches(mut cache: ResMut<ChunkStateCache>) {
    cache.visible_chunks.clear();
    cache.camera_view_chunks.clear();
}

/// Updates visible and explored chunk sets based on VisionSource positions.
/// 根据 VisionSource 位置更新可见和已探索的区块集合。
///
/// This function is the core visibility calculation system that determines which
/// chunks should be visible based on the position and range of all active vision sources.
/// It uses efficient spatial partitioning to only check chunks within the bounding
/// box of each vision source.
///
/// # Algorithm
/// For each enabled vision source:
/// 1. Calculate bounding box around the vision range
/// 2. Convert world coordinates to chunk coordinates
/// 3. Test intersection between vision circle and chunk rectangles
/// 4. Mark intersecting chunks as both visible and explored
///
/// # Performance Optimizations
/// - **Spatial Culling**: Only tests chunks within vision source bounding box
/// - **Circle-Rectangle Intersection**: Efficient geometric test
/// - **Early Termination**: Skips disabled vision sources
///
/// # Complexity Analysis
/// - **Time**: O(V × C) where V=vision sources, C=average chunks per vision range
/// - **Space**: O(E) where E=total explored chunks (accumulated over time)
/// - **Per Vision Source**: O((2R/S)²) where R=range, S=chunk size
///
/// # Visibility Rules
/// - Chunks intersecting any vision source become visible and explored
/// - Once explored, chunks remain in the explored set permanently
/// - Visibility is recalculated each frame based on current vision source positions
///
/// # Dependencies
/// - Must run after `clear_per_frame_caches`
/// - Must run before `update_chunk_component_state`
fn update_chunk_visibility(
    settings: Res<FogMapSettings>,
    mut cache: ResMut<ChunkStateCache>,
    vision_sources: Query<(&GlobalTransform, &VisionSource)>,
    // We update the cache first, then sync to components if needed
    // 我们先更新缓存，如果需要再同步到组件
) {
    let chunk_size = settings.chunk_size.as_vec2();

    for (transform, source) in vision_sources.iter() {
        if !source.enabled {
            continue;
        }

        let source_pos = transform.translation().truncate(); // Get 2D position / 获取 2D 位置
        let range_sq = source.range * source.range;

        // Calculate the bounding box of the vision circle in chunk coordinates
        // 计算视野圆形在区块坐标系下的包围盒
        let min_world = source_pos - Vec2::splat(source.range);
        let max_world = source_pos + Vec2::splat(source.range);

        let min_chunk = (min_world / chunk_size).floor().as_ivec2();
        let max_chunk = (max_world / chunk_size).ceil().as_ivec2();

        // Iterate over potentially affected chunks
        // 遍历可能受影响的区块
        for y in min_chunk.y..=max_chunk.y {
            for x in min_chunk.x..=max_chunk.x {
                let chunk_coords = IVec2::new(x, y);
                let chunk_min = chunk_coords.as_vec2() * chunk_size;
                let chunk_max = chunk_min + chunk_size;

                // Check if circle intersects chunk rectangle
                // 检查圆是否与区块矩形相交
                if circle_intersects_rect(source_pos, range_sq, chunk_min, chunk_max) {
                    // Mark as visible and explored in the cache
                    // 在缓存中标记为可见和已探索
                    cache.visible_chunks.insert(chunk_coords);
                    cache.explored_chunks.insert(chunk_coords);
                }
            }
        }
    }
}

/// Updates the set of chunks currently within the camera's view.
/// 更新当前在相机视野内的区块集合。
fn update_camera_view_chunks(
    settings: Res<FogMapSettings>,
    mut cache: ResMut<ChunkStateCache>,
    // Assuming a single primary 2D camera with OrthographicProjection
    // 假设有一个带 OrthographicProjection 的主 2D 相机
    camera_q: Query<(&Camera, &GlobalTransform, &Projection)>,
) {
    let chunk_size = settings.chunk_size.as_vec2();

    for (camera, cam_transform, projection) in camera_q.iter() {
        if let Projection::Orthographic(projection) = projection {
            // Consider only the active camera targeting the primary window
            // 只考虑渲染到主窗口的活动相机
            if !camera.is_active || !matches!(camera.target, RenderTarget::Window(_)) {
                continue;
            }

            // Calculate camera's view AABB in world space
            // 计算相机在世界空间中的视图 AABB
            // Note: This is simplified. Real calculation depends on projection type and camera orientation.
            // 注意: 这是简化的。实际计算取决于投影类型和相机方向。
            // For Orthographic, it's roughly based on scale and position.
            // 对于正交投影，大致基于缩放和位置。
            let camera_pos = cam_transform.translation().truncate();

            // 基于投影缩放和视口大小估算半宽/高 (Bevy 0.12+ 在 OrthographicProjection 中使用 `area`)
            let half_width = projection.area.width() * 0.5 * projection.scale;
            let half_height = projection.area.height() * 0.5 * projection.scale;

            let cam_min_world = camera_pos - Vec2::new(half_width, half_height);
            let cam_max_world = camera_pos + Vec2::new(half_width, half_height);

            let min_chunk = (cam_min_world / chunk_size).floor().as_ivec2();
            let max_chunk = (cam_max_world / chunk_size).ceil().as_ivec2();

            for y in min_chunk.y..=max_chunk.y {
                for x in min_chunk.x..=max_chunk.x {
                    cache.camera_view_chunks.insert(IVec2::new(x, y));
                }
            }
            // Only process one main camera / 只处理一个主相机
            break;
        }
    }
}

/// Updates the FogChunk component's state based on the cache.
/// 根据缓存更新 FogChunk 组件的状态。
fn update_chunk_component_state(
    cache: Res<ChunkStateCache>,
    chunk_manager: Res<ChunkEntityManager>,
    mut chunk_q: Query<&mut FogChunk>,
    mut snapshot_event_writer: EventWriter<RequestChunkSnapshot>, // Changed to EventWriter / 更改为 EventWriter
) {
    for (coords, entity) in chunk_manager.map.iter() {
        if let Ok(mut chunk) = chunk_q.get_mut(*entity) {
            let is_visible = cache.visible_chunks.contains(coords);
            let is_explored = cache.explored_chunks.contains(coords); // Should always contain visible

            let new_visibility = if is_visible {
                ChunkVisibility::Visible
            } else if is_explored {
                ChunkVisibility::Explored
            } else {
                ChunkVisibility::Unexplored // Should not happen if explored_chunks is managed correctly / 如果 explored_chunks 管理正确则不应发生
            };

            let old_visibility = chunk.state.visibility;
            if old_visibility != new_visibility {
                // info!("Chunk {:?} visibility changed from {:?} to {:?}", coords, old_visibility, new_visibility);
                chunk.state.visibility = new_visibility;

                // If the chunk was unexplored and is now explored/visible, OR if it was explored and is now visible, send a snapshot request event.
                // 如果区块之前是未探索状态，现在变为已探索/可见状态，或者之前是已探索状态，现在变为可见状态，则发送快照请求事件。
                let should_request_snapshot = (old_visibility == ChunkVisibility::Unexplored
                    && (new_visibility == ChunkVisibility::Explored
                        || new_visibility == ChunkVisibility::Visible))
                    || (old_visibility == ChunkVisibility::Explored
                        && new_visibility == ChunkVisibility::Visible);

                if should_request_snapshot {
                    if chunk.snapshot_layer_index.is_some() {
                        // Check if index exists before unwrapping or logging
                        let reason = if old_visibility == ChunkVisibility::Unexplored {
                            "became explored/visible"
                        } else {
                            "re-entered visibility"
                        };
                        trace!(
                            "Chunk {:?} ({}) {} ({} -> {}). Sending RequestChunkSnapshot.",
                            *coords,
                            entity.index(),
                            reason,
                            old_visibility,
                            new_visibility
                        );
                        snapshot_event_writer.write(RequestChunkSnapshot(*coords));
                    } else {
                        warn!(
                            "Chunk {:?} ({}) changed visibility ({} -> {}), but has no snapshot_layer_index. Cannot request snapshot via event.",
                            *coords,
                            entity.index(),
                            old_visibility,
                            new_visibility
                        );
                    }
                }
            }
        }
    }
}

/// Creates/activates FogChunk entities based on visibility and camera view.
/// 根据可见性和相机视图创建/激活 FogChunk 实体。
fn manage_chunk_entities(
    mut commands: Commands,
    settings: Res<FogMapSettings>,
    mut cache: ResMut<ChunkStateCache>,
    mut chunk_manager: ResMut<ChunkEntityManager>,
    mut texture_manager: ResMut<TextureArrayManager>,
    mut images: ResMut<Assets<Image>>,
    mut chunk_q: Query<&mut FogChunk>,
) {
    let chunk_size_f = settings.chunk_size.as_vec2();

    // Determine chunks that should be active (in GPU memory)
    // 确定哪些区块应该是活动的 (在 GPU 内存中)
    // Rule: Visible chunks OR explored chunks within camera view (plus buffer?)
    // 规则: 可见区块 或 相机视图内的已探索区块 (加缓冲区?)
    let mut required_gpu_chunks = cache.visible_chunks.clone();
    for coords in &cache.camera_view_chunks {
        if cache.explored_chunks.contains(coords) {
            required_gpu_chunks.insert(*coords);
        }
    }
    // Optional: Add a buffer zone around camera/visible chunks
    // 可选: 在相机/可见区块周围添加缓冲区

    // Activate/Create necessary chunks
    // 激活/创建必要的区块
    let mut chunks_to_make_gpu = HashSet::new();
    for &coords in &required_gpu_chunks {
        if let Some(entity) = chunk_manager.map.get(&coords) {
            // Chunk entity exists, check its memory state
            // 区块实体存在，检查其内存状态

            if let Ok(chunk) = chunk_q.get_mut(*entity) {
                if chunk.state.memory_location == ChunkMemoryLocation::Cpu {
                    // Mark for transition to GPU
                    // 标记以转换到 GPU
                    chunks_to_make_gpu.insert(coords);
                    // Actual data upload handled in manage_chunk_memory_logic or RenderApp
                    // 实际数据上传在 manage_chunk_memory_logic 或 RenderApp 中处理
                    // Ensure it's marked as GPU resident in cache (will be done in memory logic)
                    // 确保在缓存中标记为 GPU 驻留 (将在内存逻辑中完成)
                }
            }
        } else {
            // Chunk entity doesn't exist, create it
            // 区块实体不存在，创建它
            if let Some((fog_idx, snap_idx)) = texture_manager.allocate_layer_indices(coords) {
                let world_min = coords.as_vec2() * chunk_size_f;
                let world_bounds = Rect::from_corners(world_min, world_min + chunk_size_f);

                let mut find = false;

                for chunk in chunk_q.iter() {
                    if chunk.coords == coords {
                        find = true;
                        break;
                    }
                }

                // Check if data exists in CPU storage (was previously offloaded)
                // 检查 CPU 存储中是否存在数据 (之前被卸载过)
                let initial_state = if find {
                    // Will be loaded from CPU, mark for transition
                    // 将从 CPU 加载，标记转换
                    chunks_to_make_gpu.insert(coords);
                    ChunkState {
                        // Visibility should be Explored if it was offloaded
                        // 如果被卸载过，可见性应该是 Explored
                        visibility: ChunkVisibility::Explored,
                        memory_location: ChunkMemoryLocation::Cpu, // Will be set to Gpu by memory logic / 将由内存逻辑设为 Gpu
                    }
                } else {
                    ChunkState {
                        visibility: ChunkVisibility::Unexplored,
                        memory_location: ChunkMemoryLocation::Gpu,
                    }
                };

                let entity = commands
                    .spawn((
                        FogChunk {
                            coords,
                            fog_layer_index: Some(fog_idx),
                            snapshot_layer_index: Some(snap_idx),
                            state: initial_state,
                            world_bounds,
                        },
                        FogChunkImage::from_setting(&mut images, &settings),
                    ))
                    .id();

                chunk_manager.map.insert(coords, entity);
                cache.gpu_resident_chunks.insert(coords);

            // info!("Created FogChunk {:?} (Fog: {}, Snap: {}) State: Unexplored/Gpu. Queued initial unexplored data upload.", coords, fog_idx, snap_idx);
            } else {
                error!(
                    "Failed to allocate texture layers for chunk {:?}! TextureArray might be full.",
                    coords
                );
                // Handle error: maybe stop creating chunks, or implement LRU eviction
                // 处理错误: 可能停止创建区块，或实现 LRU 驱逐
            }
        }
    }
}

/// Efficiently tests if a circle intersects with an axis-aligned rectangle.
/// 高效测试圆形是否与轴对齐矩形相交。
///
/// This function implements the standard circle-rectangle intersection algorithm
/// by finding the closest point on the rectangle to the circle center and
/// comparing the distance to the circle radius.
///
/// # Algorithm
/// 1. Clamp circle center to rectangle bounds (finds closest point)
/// 2. Calculate squared distance from circle center to closest point
/// 3. Compare with squared radius (avoids expensive sqrt operation)
///
/// # Parameters
/// - `circle_center`: Center position of the vision source
/// - `range_sq`: Squared radius of the vision source (for performance)
/// - `rect_min`: Bottom-left corner of the chunk rectangle
/// - `rect_max`: Top-right corner of the chunk rectangle
///
/// # Performance
/// - **Complexity**: O(1) - Constant time geometric calculation
/// - **Optimizations**: Uses squared distance to avoid sqrt
/// - **Precision**: Works with f32 precision, suitable for game coordinates
///
/// # Returns
/// `true` if the circle intersects or overlaps the rectangle, `false` otherwise
///
/// # Mathematical Foundation
/// Based on the principle that the closest point on a rectangle to any external
/// point can be found by clamping coordinates to the rectangle's bounds.
fn circle_intersects_rect(
    circle_center: Vec2,
    range_sq: f32,
    rect_min: Vec2,
    rect_max: Vec2,
) -> bool {
    // Clamp the circle center to the rectangle's bounds
    // 将圆心限制在矩形边界内
    let closest_x = circle_center.x.clamp(rect_min.x, rect_max.x);
    let closest_y = circle_center.y.clamp(rect_min.y, rect_max.y);

    // Calculate the distance from the circle center to the closest point
    // 计算圆心到最近点的距离
    let dx = circle_center.x - closest_x;
    let dy = circle_center.y - closest_y;

    // If the distance is less than or equal to the radius, they intersect
    // 如果距离小于等于半径，则相交
    (dx * dx + dy * dy) <= range_sq
}

/// Orchestrates intelligent memory management between CPU and GPU for chunk textures.
/// 协调区块纹理在CPU和GPU之间的智能内存管理。
///
/// This system implements a sophisticated memory management strategy that dynamically
/// moves chunk texture data between CPU and GPU memory based on visibility requirements.
/// It optimizes memory usage by keeping only necessary chunks in expensive GPU memory
/// while preserving explored chunk data in CPU memory.
///
/// # Memory Management Strategy
///
/// ## GPU Memory Priority (chunks kept in GPU):
/// 1. **Visible chunks** - Currently within vision source range
/// 2. **Camera view explored chunks** - Explored chunks within camera viewport
/// 3. **Buffer zone chunks** - Explored chunks within 2-chunk radius of other targets
///
/// ## Transfer Triggers
/// - **CPU → GPU**: When chunks become visible or enter camera view
/// - **GPU → CPU**: When explored chunks leave target areas (preserves data)
/// - **Direct release**: When unexplored chunks are no longer needed
///
/// # Event Processing
/// The system processes several types of events:
/// - `ChunkGpuDataReady` - GPU→CPU transfer completion
/// - `ChunkCpuDataUploaded` - CPU→GPU transfer completion
///
/// # Performance Characteristics
/// - **Complexity**: O(C + E) where C=active chunks, E=transfer events
/// - **Memory Efficiency**: Minimizes GPU memory usage while preserving data
/// - **Transfer Cost**: Only moves chunks when necessary, with intelligent buffering
///
/// # Buffer Zone Strategy
/// Maintains a 2-chunk radius buffer around explored areas to ensure smooth
/// rendering during camera movement and reduce transfer frequency.
///
/// # Dependencies
/// - Processes `ChunkGpuDataReady` and `ChunkCpuDataUploaded` events
/// - Modifies chunk memory location states and layer indices
/// - Queues new transfer requests via `GpuToCpuCopyRequests` and `CpuToGpuCopyRequests`
/// - Triggers snapshot requests for chunks being moved from GPU to CPU
#[allow(clippy::too_many_arguments)]
pub fn manage_chunk_texture_transfer(
    mut commands: Commands,
    mut chunk_query: Query<(Entity, &mut FogChunk, &mut FogChunkImage)>,
    chunk_cache: Res<ChunkStateCache>,
    mut images: ResMut<Assets<Image>>,
    mut texture_manager: ResMut<TextureArrayManager>,
    mut gpu_to_cpu_requests: ResMut<GpuToCpuCopyRequests>,
    mut cpu_to_gpu_requests: ResMut<CpuToGpuCopyRequests>,
    mut gpu_data_ready_reader: EventReader<ChunkGpuDataReady>,
    mut cpu_data_uploaded_reader: EventReader<ChunkCpuDataUploaded>,
    mut snapshot_requests: ResMut<MainWorldSnapshotRequestQueue>,
) {
    for event in gpu_data_ready_reader.read() {
        if let Some((_entity, mut chunk, chunk_image)) = chunk_query
            .iter_mut()
            .find(|(_, c, _)| c.coords == event.chunk_coords)
        {
            if chunk.state.memory_location == ChunkMemoryLocation::PendingCopyToCpu {
                trace!(
                    "Chunk {:?}: GPU->CPU copy complete. Storing in CPU. Layers F{}, S{}",
                    event.chunk_coords,
                    chunk.fog_layer_index.unwrap(),
                    chunk.snapshot_layer_index.unwrap()
                );
                let fog_image = images
                    .get_mut(&chunk_image.fog_image_handle)
                    .expect("Failed to get fog image");
                fog_image.data = Some(event.fog_data.clone());
                let snapshot_image = images
                    .get_mut(&chunk_image.snapshot_image_handle)
                    .expect("Failed to get snapshot image");
                snapshot_image.data = Some(event.snapshot_data.clone());

                // if let Some((fog_data, snapshot_data)) = cpu_storage.storage.get(&chunk.coords) {} else {
                //     let fog_image = Image::new_fill(Extent3d {
                //         width: settings.chunk_size.x,
                //         height: settings.chunk_size.y,
                //         depth_or_array_layers: 1,
                //     }, /* &[u8] */, /* bevy::bevy_render::render_resource::TextureFormat */ /* RenderAssetUsages */)
                //
                //     cpu_storage.storage.insert(
                //         event.chunk_coords,
                //         (event.fog_data.clone(), event.snapshot_data.clone()),
                //     );
                //
                // }

                // 释放 TextureArray 层索引
                // Free TextureArray layer indices
                texture_manager.free_layer_indices_for_coord(chunk.coords);
                chunk.fog_layer_index = None;
                chunk.snapshot_layer_index = None;
                chunk.state.memory_location = ChunkMemoryLocation::Cpu;
            } else {
                warn!(
                    "Chunk {:?}: Received GpuDataReady but state is {:?}, expected PendingCopyToCpu.",
                    event.chunk_coords, chunk.state.memory_location
                );
            }
        } else {
            warn!(
                "Received GpuDataReady for unknown chunk: {:?}",
                event.chunk_coords
            );
        }
    }

    for event in cpu_data_uploaded_reader.read() {
        if let Some((_entity, mut chunk, mut _chunk_image)) = chunk_query
            .iter_mut()
            .find(|(_, c, _)| c.coords == event.chunk_coords)
        {
            if chunk.state.memory_location == ChunkMemoryLocation::PendingCopyToGpu {
                trace!(
                    "Chunk {:?}: CPU->GPU upload complete. Now resident on GPU. Layers F{}, S{}.",
                    event.chunk_coords,
                    chunk.fog_layer_index.unwrap(),
                    chunk.snapshot_layer_index.unwrap()
                );
                chunk.state.memory_location = ChunkMemoryLocation::Gpu;
            } else {
                warn!(
                    "Chunk {:?}: Received CpuDataUploaded but state is {:?}, expected PendingCopyToGpu.",
                    event.chunk_coords, chunk.state.memory_location
                );
            }
        } else {
            warn!(
                "Received CpuDataUploaded for unknown chunk: {:?}",
                event.chunk_coords
            );
        }
    }

    // 清空CPU到GPU请求队列，因为它们将被重新评估
    // Clear CPU-to-GPU request queue as they will be re-evaluated
    // NOTE: GPU-to-CPU requests are cleared by the render world after processing
    cpu_to_gpu_requests.requests.clear();

    // --- 2. 决定哪些区块应该在 GPU 上 ---
    // --- 2. Decide which chunks should be on GPU ---
    let mut target_gpu_chunks = HashSet::new();
    // 可见区块必须在 GPU
    // Visible chunks must be on GPU
    for &coords in &chunk_cache.visible_chunks {
        target_gpu_chunks.insert(coords);
    }
    // 在相机视野内且已探索的区块也应该在 GPU
    // Explored chunks within camera view should also be on GPU
    for &coords in &chunk_cache.camera_view_chunks {
        if chunk_cache.explored_chunks.contains(&coords) {
            target_gpu_chunks.insert(coords);
        }
    }

    // 为已探索区域添加缓冲区，确保持久化加载后的区域能被渲染
    // Add buffer zone for explored areas to ensure persistence-loaded areas are rendered
    let buffer_radius = 2; // 缓冲区半径（以chunk为单位）/ Buffer radius in chunks
    let mut buffered_coords = std::collections::HashSet::new();

    // 为所有已探索的chunk添加缓冲区
    // Add buffer zone around all explored chunks
    for &explored_coord in &chunk_cache.explored_chunks {
        for dy in -buffer_radius..=buffer_radius {
            for dx in -buffer_radius..=buffer_radius {
                let buffered_coord = explored_coord + IVec2::new(dx, dy);
                if chunk_cache.explored_chunks.contains(&buffered_coord) {
                    buffered_coords.insert(buffered_coord);
                }
            }
        }
    }

    // 将缓冲区内的已探索chunk添加到GPU目标列表
    // Add buffered explored chunks to GPU target list
    for &coords in &buffered_coords {
        target_gpu_chunks.insert(coords);
    }

    // --- 3. 遍历所有区块，确定是否需要传输 ---
    // --- 3. Iterate all chunks to determine if transfer is needed ---
    for (entity, mut chunk, chunk_image) in chunk_query.iter_mut() {
        let should_be_on_gpu = target_gpu_chunks.contains(&chunk.coords);

        match chunk.state.memory_location {
            ChunkMemoryLocation::Gpu => {
                if !should_be_on_gpu && chunk.state.visibility == ChunkVisibility::Explored {
                    // 条件：在 GPU 上，但不再需要，并且是已探索状态 (值得保存)
                    // Condition: On GPU, but no longer needed, and is Explored (worth saving)
                    if let (Some(fog_idx_val), Some(snap_idx_val)) =
                        (chunk.fog_layer_index, chunk.snapshot_layer_index)
                    {
                        trace!(
                            "Chunk {:?}: Requesting GPU -> CPU transfer (is Explored, not target GPU). Layers F{}, S{}",
                            chunk.coords, fog_idx_val, snap_idx_val
                        );
                        snapshot_requests.requests.push(MainWorldSnapshotRequest {
                            chunk_coords: chunk.coords,
                            snapshot_layer_index: snap_idx_val,
                            world_bounds: chunk.world_bounds,
                        });

                        chunk.state.memory_location = ChunkMemoryLocation::PendingCopyToCpu;
                        gpu_to_cpu_requests.requests.push(GpuToCpuCopyRequest {
                            chunk_coords: chunk.coords,
                            fog_layer_index: fog_idx_val, // Pass the unwrapped value
                            snapshot_layer_index: snap_idx_val,
                        });
                        // 索引在 GpuDataReady 事件处理中设为 None
                        // Indices are set to None in GpuDataReady event handling
                    } else {
                        warn!(
                            "Chunk {:?}: Wanted GPU->CPU but indices are None. State: {:?}, Visibility: {:?}",
                            chunk.coords, chunk.state, chunk.state.visibility
                        );
                    }
                } else if !should_be_on_gpu && chunk.state.visibility == ChunkVisibility::Unexplored
                {
                    // 条件：在 GPU 上，但不再需要，并且是未探索状态 (不需要保存，直接释放)
                    // Condition: On GPU, but no longer needed, and is Unexplored (no need to save, just free)
                    // 这种情况通常由 manage_chunk_entities 通过销毁实体来处理
                    // This case is usually handled by manage_chunk_entities by despawning the entity
                    // 如果实体仍然存在，我们在这里释放层
                    // If entity still exists, we free layers here
                    trace!(
                        "Chunk {:?}: Unexplored and no longer target for GPU. Freeing layers.",
                        chunk.coords
                    );
                    texture_manager.free_layer_indices_for_coord(chunk.coords);
                    // 考虑直接销毁此实体或标记以便 manage_chunk_entities 处理
                    // Consider despawning this entity directly or marking it for manage_chunk_entities
                    chunk.state.memory_location = ChunkMemoryLocation::Cpu; // Or a new "Freed" state
                    commands.entity(entity).remove::<FogChunk>(); // Example: Despawn
                    // Note: this requires removing from ChunkEntityManager too
                }
            }
            ChunkMemoryLocation::Cpu => {
                if should_be_on_gpu {
                    // 条件：在 CPU 上，但现在需要上 GPU
                    // Condition: On CPU, but now needed on GPU

                    // Check if chunk already has allocated layer indices (e.g., from persistence loading)
                    // 检查区块是否已经有分配的层索引（例如，从持久化加载）
                    let (fog_idx_val, snap_idx_val) = if let (Some(fog_idx), Some(snap_idx)) =
                        (chunk.fog_layer_index, chunk.snapshot_layer_index)
                    {
                        // Use existing layer indices
                        // 使用现有的层索引
                        trace!(
                            "Chunk {:?}: Using existing layer indices F{}, S{} for CPU -> GPU transfer",
                            chunk.coords, fog_idx, snap_idx
                        );
                        (fog_idx, snap_idx)
                    } else if let Some((fog_idx, snap_idx)) =
                        texture_manager.allocate_layer_indices(chunk.coords)
                    {
                        // Allocate new layer indices
                        // 分配新的层索引
                        trace!(
                            "Chunk {:?}: Allocated new layer indices F{}, S{} for CPU -> GPU transfer",
                            chunk.coords, fog_idx, snap_idx
                        );
                        chunk.fog_layer_index = Some(fog_idx);
                        chunk.snapshot_layer_index = Some(snap_idx);
                        (fog_idx, snap_idx)
                    } else {
                        warn!(
                            "Chunk {:?}: Wanted to move CPU -> GPU, but no free texture layers!",
                            chunk.coords
                        );
                        continue; // Skip this chunk and continue with the next one
                    };

                    chunk.state.memory_location = ChunkMemoryLocation::PendingCopyToGpu;
                    cpu_to_gpu_requests.requests.push(CpuToGpuCopyRequest {
                        chunk_coords: chunk.coords,
                        fog_layer_index: fog_idx_val,
                        snapshot_layer_index: snap_idx_val,
                        fog_image_handle: chunk_image.fog_image_handle.clone(),
                        snapshot_image_handle: chunk_image.snapshot_image_handle.clone(),
                    });

                    // 从 CPU 存储中移除，因为它正在被上传
                    // Remove from CPU storage as it's being uploaded
                    // (可选：可以等到 ChunkCpuDataUploaded 再移除，以防上传失败)
                    // (Optional: can wait for ChunkCpuDataUploaded before removing, in case upload fails)
                    // cpu_storage.storage.remove(&chunk.coords);
                }
            }
            ChunkMemoryLocation::PendingCopyToCpu | ChunkMemoryLocation::PendingCopyToGpu => {
                // 正在传输中，等待事件
                // In transit, waiting for event
            }
        }
    }
}

/// 重置雾效系统的所有状态，包括已探索区域、可见性状态和纹理数据。
/// Reset all fog of war system state, including explored areas, visibility states, and texture data.
/// 重构为4个参数以减少耦合。
/// Refactored to 4 parameters to reduce coupling.
#[allow(clippy::too_many_arguments)]
fn reset_fog_of_war_system(
    mut events: EventReader<ResetFogOfWar>,
    mut cache: ResMut<ChunkStateCache>,
    mut chunk_q: Query<&mut FogChunk>,
    mut chunk_query: Query<(Entity, &mut FogChunkImage)>,
    mut texture_manager: ResMut<TextureArrayManager>,
    mut images: ResMut<Assets<Image>>,
    fog_texture: Res<FogTextureArray>,
    visibility_texture: Res<VisibilityTextureArray>,
    snapshot_texture: Res<SnapshotTextureArray>,
    _settings: Res<FogMapSettings>,
    mut commands: Commands,
    mut chunk_manager: ResMut<ChunkEntityManager>,
    mut reset_sync: ResMut<FogResetSync>,
    time: Res<Time>,
) {
    for _event in events.read() {
        // 检查是否已有重置正在进行
        // Check if reset is already in progress
        if reset_sync.state != ResetSyncState::Idle {
            warn!("Reset already in progress, state: {:?}", reset_sync.state);
            continue;
        }

        info!("Starting atomic fog of war reset...");
        let current_time = time.elapsed().as_millis() as u64;

        // 创建详细的检查点用于回滚
        // Create detailed checkpoint for rollback
        let checkpoint = ResetCheckpoint {
            explored_chunks: cache.explored_chunks.clone(),
            visible_chunks: cache.visible_chunks.clone(),
            gpu_resident_chunks: cache.gpu_resident_chunks.clone(),
            camera_view_chunks: cache.camera_view_chunks.clone(),
            created_at: current_time,
        };

        reset_sync.checkpoint = Some(checkpoint);
        reset_sync.chunks_count = chunk_manager.map.len();

        // 执行主世界重置操作
        // Execute main world reset operations
        if let Err(error) = execute_main_world_reset(
            &mut cache,
            &mut chunk_q,
            &mut chunk_query,
            &mut texture_manager,
            &mut images,
            &fog_texture,
            &visibility_texture,
            &snapshot_texture,
            &mut commands,
            &mut chunk_manager,
        ) {
            error!("Main world reset failed: {}", error);

            // 尝试回滚到检查点
            // Try to rollback to checkpoint
            if let Some(checkpoint) = reset_sync.get_checkpoint() {
                match rollback_reset_to_checkpoint(&mut cache, checkpoint) {
                    Ok(()) => {
                        warn!(
                            "Successfully rolled back to checkpoint after main world reset failure"
                        );
                    }
                    Err(rollback_error) => {
                        error!(
                            "Failed to rollback after main world reset failure: {}",
                            rollback_error
                        );
                    }
                }
            }

            reset_sync.mark_failed(error);
            continue;
        }

        // 标记主世界重置完成，开始渲染世界同步
        // Mark main world reset complete, start render world sync
        reset_sync.start_reset(current_time);

        info!("Main world reset complete, waiting for render world sync...");
    }
}

/// 执行主世界重置操作，返回详细错误信息
/// Execute main world reset operations, returns detailed error information
#[allow(clippy::too_many_arguments)]
fn execute_main_world_reset(
    cache: &mut ResMut<ChunkStateCache>,
    chunk_q: &mut Query<&mut FogChunk>,
    chunk_query: &mut Query<(Entity, &mut FogChunkImage)>,
    texture_manager: &mut ResMut<TextureArrayManager>,
    images: &mut ResMut<Assets<Image>>,
    fog_texture: &Res<FogTextureArray>,
    visibility_texture: &Res<VisibilityTextureArray>,
    snapshot_texture: &Res<SnapshotTextureArray>,
    commands: &mut Commands,
    chunk_manager: &mut ResMut<ChunkEntityManager>,
) -> Result<(), FogResetError> {
    // 将复杂的重置逻辑拆分为更小的函数，每个都能返回具体错误
    // Break down complex reset logic into smaller functions, each can return specific errors
    reset_chunk_cache(cache).map_err(FogResetError::CacheResetFailed)?;

    reset_chunk_states(chunk_q, texture_manager).map_err(FogResetError::ChunkStateResetFailed)?;

    reset_chunk_images(chunk_query, images).map_err(FogResetError::ImageResetFailed)?;

    reset_main_textures(images, fog_texture, visibility_texture, snapshot_texture)
        .map_err(FogResetError::TextureResetFailed)?;

    cleanup_chunk_entities(chunk_manager, commands).map_err(FogResetError::EntityCleanupFailed)?;

    info!("Main world reset operations completed successfully");
    Ok(())
}

/// 重置区块缓存状态
/// Reset chunk cache state
fn reset_chunk_cache(cache: &mut ResMut<ChunkStateCache>) -> Result<(), String> {
    let explored_count = cache.explored_chunks.len();
    let visible_count = cache.visible_chunks.len();
    let gpu_count = cache.gpu_resident_chunks.len();

    // 验证缓存状态
    // Validate cache state
    if explored_count > 10000 || visible_count > 10000 || gpu_count > 10000 {
        return Err(format!(
            "Cache sizes too large: explored={explored_count}, visible={visible_count}, gpu={gpu_count}"
        ));
    }

    cache.reset_all();
    info!(
        "Reset cache: {} explored, {} visible, {} gpu chunks cleared",
        explored_count, visible_count, gpu_count
    );
    Ok(())
}

/// 重置区块状态
/// Reset chunk states
fn reset_chunk_states(
    chunk_q: &mut Query<&mut FogChunk>,
    texture_manager: &mut ResMut<TextureArrayManager>,
) -> Result<(), String> {
    let chunk_count = chunk_q.iter().count();

    // 验证区块数量
    // Validate chunk count
    if chunk_count > 1000 {
        return Err(format!("Too many chunks to reset: {chunk_count}"));
    }

    for mut chunk in chunk_q.iter_mut() {
        chunk.state.visibility = ChunkVisibility::Unexplored;
        chunk.state.memory_location = ChunkMemoryLocation::Cpu;
        chunk.fog_layer_index = None;
        chunk.snapshot_layer_index = None;
    }
    info!("Reset {} chunk states to Unexplored/Cpu", chunk_count);

    // 清除所有纹理层分配
    // Clear all texture layer allocations
    texture_manager.clear_all_layers();
    Ok(())
}

/// 重置区块图像数据
/// Reset chunk image data
fn reset_chunk_images(
    chunk_query: &mut Query<(Entity, &mut FogChunkImage)>,
    images: &mut ResMut<Assets<Image>>,
) -> Result<(), String> {
    for (_entity, chunk_image) in chunk_query.iter_mut() {
        if let Some(fog_image) = images.get_mut(&chunk_image.fog_image_handle) {
            // 使用统一的纹理大小计算器
            // Use unified texture size calculator
            let size_info = TextureSizeCalculator::calculate_2d_single_channel(
                fog_image.texture_descriptor.size.width,
                fog_image.texture_descriptor.size.height,
            )
            .map_err(|e| format!("Failed to calculate fog texture size: {e}"))?;

            fog_image.data = Some(vec![0u8; size_info.total_bytes]);
        }
        if let Some(snapshot_image) = images.get_mut(&chunk_image.snapshot_image_handle) {
            // 使用统一的纹理大小计算器（RGBA）
            // Use unified texture size calculator (RGBA)
            let size_info = TextureSizeCalculator::calculate_2d_rgba(
                snapshot_image.texture_descriptor.size.width,
                snapshot_image.texture_descriptor.size.height,
            )
            .map_err(|e| format!("Failed to calculate snapshot texture size: {e}"))?;

            snapshot_image.data = Some(vec![0u8; size_info.total_bytes]);
        }
    }
    Ok(())
}

/// 重置主纹理数据
/// Reset main texture data
fn reset_main_textures(
    images: &mut ResMut<Assets<Image>>,
    fog_texture: &Res<FogTextureArray>,
    visibility_texture: &Res<VisibilityTextureArray>,
    snapshot_texture: &Res<SnapshotTextureArray>,
) -> Result<(), String> {
    // Reset fog texture
    if let Some(fog_image) = images.get_mut(&fog_texture.handle) {
        // 使用统一的纹理大小计算器（3D单通道）
        // Use unified texture size calculator (3D single channel)
        let size_info = TextureSizeCalculator::calculate_3d_single_channel(
            fog_image.texture_descriptor.size.width,
            fog_image.texture_descriptor.size.height,
            fog_image.texture_descriptor.size.depth_or_array_layers,
        )
        .map_err(|e| format!("Failed to calculate main fog texture size: {e}"))?;

        fog_image.data = Some(vec![0u8; size_info.total_bytes]);
        info!("Reset fog texture data: {} bytes", size_info.total_bytes);
    }

    // Reset visibility texture
    if let Some(visibility_image) = images.get_mut(&visibility_texture.handle) {
        // 使用统一的纹理大小计算器（3D单通道）
        // Use unified texture size calculator (3D single channel)
        let size_info = TextureSizeCalculator::calculate_3d_single_channel(
            visibility_image.texture_descriptor.size.width,
            visibility_image.texture_descriptor.size.height,
            visibility_image
                .texture_descriptor
                .size
                .depth_or_array_layers,
        )
        .map_err(|e| format!("Failed to calculate main visibility texture size: {e}"))?;

        visibility_image.data = Some(vec![0u8; size_info.total_bytes]);
        info!(
            "Reset visibility texture data: {} bytes",
            size_info.total_bytes
        );
    }

    // Reset snapshot texture
    if let Some(snapshot_image) = images.get_mut(&snapshot_texture.handle) {
        // 使用统一的纹理大小计算器（3D RGBA）
        // Use unified texture size calculator (3D RGBA)
        let size_info = TextureSizeCalculator::calculate_3d_rgba(
            snapshot_image.texture_descriptor.size.width,
            snapshot_image.texture_descriptor.size.height,
            snapshot_image.texture_descriptor.size.depth_or_array_layers,
        )
        .map_err(|e| format!("Failed to calculate main snapshot texture size: {e}"))?;

        snapshot_image.data = Some(vec![0u8; size_info.total_bytes]);
        info!(
            "Reset snapshot texture data: {} bytes",
            size_info.total_bytes
        );
    }
    Ok(())
}

/// 清理区块实体
/// Cleanup chunk entities
fn cleanup_chunk_entities(
    chunk_manager: &mut ResMut<ChunkEntityManager>,
    commands: &mut Commands,
) -> Result<(), String> {
    let entity_count = chunk_manager.map.len();

    // 验证实体数量
    // Validate entity count
    if entity_count > 1000 {
        return Err(format!("Too many entities to cleanup: {entity_count}"));
    }

    for (_coords, entity) in chunk_manager.map.iter() {
        commands.entity(*entity).despawn();
    }

    // 清除区块实体管理器映射
    // Clear the chunk entity manager mapping
    chunk_manager.map.clear();

    info!("Despawned {} chunk entities", entity_count);
    Ok(())
}

/// 回滚重置操作到检查点状态
/// Rollback reset operation to checkpoint state
fn rollback_reset_to_checkpoint(
    cache: &mut ResMut<ChunkStateCache>,
    checkpoint: &ResetCheckpoint,
) -> Result<(), FogResetError> {
    // 验证检查点数据
    // Validate checkpoint data
    if checkpoint.explored_chunks.len() > 10000
        || checkpoint.visible_chunks.len() > 10000
        || checkpoint.gpu_resident_chunks.len() > 10000
        || checkpoint.camera_view_chunks.len() > 10000
    {
        return Err(FogResetError::RollbackFailed(format!(
            "Checkpoint data too large: explored={}, visible={}, gpu={}, camera={}",
            checkpoint.explored_chunks.len(),
            checkpoint.visible_chunks.len(),
            checkpoint.gpu_resident_chunks.len(),
            checkpoint.camera_view_chunks.len()
        )));
    }

    // 恢复区块缓存状态
    // Restore chunk cache state
    cache.explored_chunks = checkpoint.explored_chunks.clone();
    cache.visible_chunks = checkpoint.visible_chunks.clone();
    cache.gpu_resident_chunks = checkpoint.gpu_resident_chunks.clone();
    cache.camera_view_chunks = checkpoint.camera_view_chunks.clone();

    info!(
        "Rollback completed: restored {} explored, {} visible, {} GPU, {} camera view chunks",
        cache.explored_chunks.len(),
        cache.visible_chunks.len(),
        cache.gpu_resident_chunks.len(),
        cache.camera_view_chunks.len()
    );

    Ok(())
}

/// 监控重置同步状态，处理超时和状态转换
/// Monitor reset sync state, handle timeouts and state transitions
fn monitor_reset_sync_system(
    mut reset_sync: ResMut<FogResetSync>,
    mut cache: ResMut<ChunkStateCache>,
    time: Res<Time>,
    mut success_events: EventWriter<FogResetSuccess>,
    mut failure_events: EventWriter<FogResetFailed>,
) {
    let current_time = time.elapsed().as_millis() as u64;

    match reset_sync.state {
        ResetSyncState::Idle => {
            // 空闲状态，无需处理
            // Idle state, no processing needed
        }
        ResetSyncState::MainWorldComplete => {
            // 由于渲染世界的 ExtractResource 状态更新不会传回主世界，
            // 我们使用一个简单的超时机制来标记完成
            // Since render world ExtractResource state updates don't propagate back to main world,
            // we use a simple timeout mechanism to mark completion
            if let Some(start_time) = reset_sync.start_time {
                let elapsed = current_time - start_time;

                // 给渲染世界足够时间完成处理（2秒）
                // Give render world enough time to complete processing (2 seconds)
                if elapsed > 2000 {
                    info!(
                        "Assuming render world processing completed after {}ms",
                        elapsed
                    );
                    reset_sync.state = ResetSyncState::Complete;
                } else if elapsed > 1000 && elapsed % 500 < 50 {
                    // Log every 500ms after 1 second
                    debug!(
                        "Waiting for render world processing... elapsed: {}ms",
                        elapsed
                    );
                }

                // 仍然保留超时保护
                // Still keep timeout protection
                if reset_sync.is_timeout(current_time) {
                    let elapsed = current_time - reset_sync.start_time.unwrap_or(current_time);
                    error!(
                        "Reset timeout waiting for render world processing after {}ms (timeout: {}ms)",
                        elapsed, reset_sync.timeout_ms
                    );
                    reset_sync.mark_failed(FogResetError::Timeout(
                        "Timeout waiting for render world processing".to_string(),
                    ));
                }
            }
        }
        ResetSyncState::RenderWorldProcessing => {
            // 检查是否超时
            // Check for timeout
            if reset_sync.is_timeout(current_time) {
                error!("Reset timeout during render world processing");
                reset_sync.mark_failed(FogResetError::Timeout(
                    "Timeout during render world processing".to_string(),
                ));
            }
        }
        ResetSyncState::Complete => {
            // 重置完成，回到空闲状态
            // Reset complete, return to idle state
            let duration_ms = current_time - reset_sync.start_time.unwrap_or(current_time);
            info!("Reset sync completed successfully in {}ms", duration_ms);

            // 发送成功事件
            // Send success event
            success_events.write(FogResetSuccess {
                duration_ms,
                chunks_reset: reset_sync.chunks_count,
            });

            reset_sync.reset_to_idle();
        }
        ResetSyncState::Failed(ref error) => {
            // 重置失败，尝试回滚到检查点
            // Reset failed, try to rollback to checkpoint
            let duration_ms = current_time - reset_sync.start_time.unwrap_or(current_time);
            error!("Reset sync failed: {} (duration: {}ms)", error, duration_ms);

            // 发送失败事件
            // Send failure event
            failure_events.write(FogResetFailed {
                error: error.clone(),
                duration_ms,
            });

            if let Some(checkpoint) = reset_sync.get_checkpoint() {
                match rollback_reset_to_checkpoint(&mut cache, checkpoint) {
                    Ok(()) => {
                        warn!("Successfully rolled back to checkpoint after reset failure");
                    }
                    Err(rollback_error) => {
                        error!("Failed to rollback to checkpoint: {}", rollback_error);
                    }
                }
            } else {
                warn!("No checkpoint available for rollback");
            }

            reset_sync.reset_to_idle();
        }
    }
}
