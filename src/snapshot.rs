use crate::prelude::*;
use crate::render::{RenderSnapshotTempTexture, RenderSnapshotTexture};
use crate::{FogSystems, RequestChunkSnapshot};
use bevy::{
    asset::RenderAssetUsages,
    core_pipeline::core_2d::graph::{Core2d, Node2d},
    render::{
        RenderApp,
        camera::RenderTarget,
        extract_component::ExtractComponent,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_graph::{Node, NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel},
        render_resource::{
            Extent3d, Origin3d, TexelCopyTextureInfo, TextureAspect, TextureDimension,
            TextureUsages,
        },
        renderer::RenderContext,
        texture::GpuImage,
        view::RenderLayers,
    },
};

/// Plugin for managing fog of war snapshot system that captures previously explored areas.
/// 管理战争迷雾快照系统的插件，该系统捕获先前探索过的区域
///
/// The snapshot system creates visual memories of previously explored areas by:
/// 1. **Camera Setup**: Creates a dedicated snapshot camera for capturing entity states
/// 2. **Render Integration**: Integrates with Bevy's render graph for texture processing
/// 3. **Request Management**: Handles snapshot creation requests from the main world
/// 4. **Layer Management**: Ensures capturable entities are on the correct render layer
///
/// # Architecture
/// ```text
/// [Main World]     [Render World]
///      │                 │
/// Snapshot Request ──────→ SnapshotNode
///      │                 │
/// Camera Setup     ──────→ Texture Copy
///      │                 │
/// Layer Management ──────→ Final Snapshot
/// ```
///
/// # Performance Characteristics
/// - **Memory**: Creates one temporary texture per chunk snapshot
/// - **GPU Usage**: Performs texture-to-texture copies via render graph
/// - **CPU Overhead**: Minimal, mostly event-driven processing
/// - **Time Complexity**: O(1) per snapshot request, O(n) for render layer updates
///
/// # Integration Points
/// - **Main Plugin**: Added as part of FogOfWarPlugin system
/// - **Render Graph**: Connects between MainTransparentPass and FogComputeNode
/// - **Asset System**: Creates and manages snapshot texture assets
/// - **ECS Systems**: Manages camera state and entity render layers
///
/// # Example Usage
/// The plugin is automatically registered when using FogOfWarPlugin:
/// ```rust,no_run
/// App::new()
///     .add_plugins(FogOfWarPlugin) // SnapshotPlugin included automatically
///     .run();
/// ```
pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    /// Configures the snapshot system by setting up resources, events, and render graph integration.
    /// 通过设置资源、事件和渲染图集成来配置快照系统
    ///
    /// # System Setup
    /// - **Startup Systems**: `setup_snapshot_camera` - Creates snapshot camera and temp texture
    /// - **Update Systems**: `prepare_snapshot_camera`, `ensure_snapshot_render_layer`, `handle_request_chunk_snapshot_events`
    /// - **PostUpdate Systems**: `prepare_snapshot_camera` - Configures camera for next frame
    /// - **Last Systems**: `check_snapshot_image_ready` - Manages camera activation state
    ///
    /// # Render Graph Integration
    /// Adds SnapshotNode to Core2d render graph between MainTransparentPass and FogComputeNode:
    /// ```text
    /// MainTransparentPass → SnapshotNode → FogComputeNode → FogOverlayNode → EndMainPass
    /// ```
    ///
    /// # Time Complexity: O(1) for setup, O(n) for entity render layer updates per frame
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<SnapshotCameraState>::default());
        app.init_resource::<SnapshotCameraState>();
        app.add_event::<RequestCleanChunkSnapshot>();
        app.add_systems(Startup, setup_snapshot_camera)
            .add_systems(PostUpdate, prepare_snapshot_camera)
            .add_systems(Update, ensure_snapshot_render_layer)
            .add_systems(Last, check_snapshot_image_ready);

        // System to handle explicit snapshot remake requests
        // 处理显式快照重制请求的系统
        app.add_systems(
            Update,
            handle_request_chunk_snapshot_events
                .after(FogSystems::UpdateChunkState) // Run after chunk states are updated / 在区块状态更新后运行
                .before(FogSystems::ManageEntities), // Before entities are managed based on new requests / 在基于新请求管理实体之前
        );

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.add_render_graph_node::<SnapshotNode>(Core2d, SnapshotNodeLabel);
        // render_app.add_render_graph_edge(Core2d, SnapshotNodeLabel, Node2d::MainTransparentPass);

        render_app.add_render_graph_edges(
            Core2d,
            (
                Node2d::MainTransparentPass,
                SnapshotNodeLabel,
                crate::render::FogComputeNodeLabel,
                crate::render::FogOverlayNodeLabel,
                Node2d::EndMainPass,
            ),
        );
    }
}

/// 由主世界填充的快照请求队列，
/// Queue populated by the main world for snapshot requests
#[derive(Resource, Default, Debug, Clone, Reflect)]
#[reflect(Resource, Default)]
pub struct MainWorldSnapshotRequestQueue {
    pub requests: Vec<MainWorldSnapshotRequest>,
}

/// Information for a single snapshot request, generated in the main world.
/// 单个快照请求的信息，在主世界中生成。
#[derive(Debug, Clone, Reflect)]
pub struct MainWorldSnapshotRequest {
    pub chunk_coords: IVec2,
    pub snapshot_layer_index: u32,
    pub world_bounds: Rect, // 区块在世界坐标系下的包围盒 / Bounding box of the chunk in world coordinates
}

/// Event to request clean a snapshot for a specific chunk.
/// 请求为特定区块清理快照的事件。
#[derive(Event, Debug, Clone, Copy)]
pub struct RequestCleanChunkSnapshot(pub IVec2);

/// 标记组件，指示该实体应被包含在战争迷雾的快照中
/// Marker component indicating this entity should be included in the fog of war snapshot
#[derive(Component, Debug, Clone, Default, Reflect)]
#[reflect(Component, Default)]
pub struct Capturable;

/// Marker component for a camera used to render snapshots.
/// 用于渲染快照的相机的标记组件。
#[derive(Component, ExtractComponent, Clone, Default, Reflect)]
#[reflect(Component)]
pub struct SnapshotCamera;

/// Component marking a camera as currently processing a specific snapshot target.
/// 将相机标记为当前正在处理特定快照目标的组件
///
/// This component is attached to snapshot cameras when they are actively capturing
/// a chunk snapshot. It provides the necessary information for proper camera
/// positioning and render target configuration.
///
/// # Component Data
/// - **snapshot_layer_index**: Target layer in snapshot texture array for final copy
/// - **world_bounds**: World-space bounds of the chunk being captured
///
/// # Usage Pattern
/// ```text
/// Camera Idle → Request Arrives → Add ActiveSnapshotTarget → Capture → Remove Component
/// ```
///
/// # Performance Notes
/// - **Lifetime**: Short-lived, typically 2-3 frames per snapshot
/// - **Memory**: Minimal - just two fields per active snapshot
/// - **Query Impact**: Enables efficient camera state queries
#[derive(Component)]
pub struct ActiveSnapshotTarget {
    /// Target texture array layer index for the final snapshot copy.
    /// 最终快照复制的目标纹理数组层索引
    pub snapshot_layer_index: u32,
    /// World-space bounds of the chunk being captured for camera positioning.
    /// 正在捕获的区块的世界空间边界，用于相机定位
    pub world_bounds: Rect, // For reference, projection is set based on this
}

/// Render layer ID used exclusively for snapshot camera visibility.
/// 专门用于快照相机可见性的渲染层ID
///
/// This layer (7) is reserved for the snapshot system to ensure clean separation
/// between main camera rendering and snapshot capture. Entities marked with
/// Capturable component are automatically added to this layer.
pub const SNAPSHOT_RENDER_LAYER_ID: usize = 7;

/// Pre-configured RenderLayers for snapshot camera visibility.
/// 为快照相机可见性预配置的渲染层
///
/// This constant provides a convenient RenderLayers instance configured for
/// snapshot rendering. The snapshot camera is configured to render only this
/// layer, ensuring isolation from main camera rendering.
pub const SNAPSHOT_RENDER_LAYER: RenderLayers = RenderLayers::layer(SNAPSHOT_RENDER_LAYER_ID);

/// Prepares and configures the single SnapshotCamera entity for the current frame's snapshot request (if any).
/// 为当前帧的快照请求（如果有）准备和配置单个 SnapshotCamera 实体。
///
/// # System Behavior
/// This system processes one snapshot request per frame from the MainWorldSnapshotRequestQueue:
/// 1. **Queue Processing**: Pops one request from the request queue
/// 2. **Camera Positioning**: Positions camera at chunk center with appropriate Z-depth
/// 3. **Camera Activation**: Enables camera and sets capturing state
/// 4. **Frame Timing**: Sets frame wait counter to allow render completion
///
/// # Performance Characteristics
/// - **Throughput**: Processes 1 snapshot request per frame maximum
/// - **Memory**: Temporary camera transform and state updates only
/// - **Time Complexity**: O(1) per frame, O(n) total for n requests
/// - **GPU Impact**: Camera activation triggers render pipeline next frame
///
/// # Camera Configuration
/// - **Position**: Centers camera on chunk world bounds center
/// - **Z-Depth**: Fixed at 999.0 to ensure visibility of 2D entities
/// - **State**: Sets capturing=true, frame_to_wait=2 for proper timing
/// - **Activation**: Enables camera.is_active for render system
///
/// # Timing Coordination
/// The 2-frame wait ensures proper synchronization:
/// - Frame N: Camera positioned and activated
/// - Frame N+1: Scene rendered to snapshot texture
/// - Frame N+2: SnapshotNode copies texture to array layer
fn prepare_snapshot_camera(
    mut snapshot_requests: ResMut<MainWorldSnapshotRequestQueue>,
    snapshot_camera_query: Single<(&mut Camera, &mut GlobalTransform), With<SnapshotCamera>>,
    mut snapshot_camera_state: ResMut<SnapshotCameraState>,
) {
    if snapshot_camera_state.capturing {
        return;
    }
    // Process one request per frame for simplicity
    let (mut camera, mut global_transform) = snapshot_camera_query.into_inner();

    if let Some(request) = snapshot_requests.requests.pop() {
        // Take one request
        debug!(
            "Preparing snapshot camera for layer {} at {:?} ",
            request.snapshot_layer_index,
            request.world_bounds.center()
        );
        let center = request.world_bounds.center();
        let transform = Transform::from_xyz(center.x, center.y, 999.0); // Ensure Z is appropriate
        *global_transform = GlobalTransform::from(transform);
        camera.is_active = true;
        snapshot_camera_state.snapshot_layer_index = Some(request.snapshot_layer_index);
        snapshot_camera_state.capturing = true;
        snapshot_camera_state.frame_to_wait = 2;
    }
}

/// Creates the snapshot camera entity and temporary texture for capturing chunk states.
/// 创建快照相机实体和用于捕获区块状态的临时纹理
///
/// # Camera Configuration
/// Sets up a specialized 2D orthographic camera for snapshot capture:
/// - **Projection**: Orthographic with scale matching chunk-to-texture ratio
/// - **Scaling**: Fixed dimensions matching texture resolution per chunk
/// - **Clear Color**: Transparent (0,0,0,0) to preserve background transparency
/// - **Render Order**: -1 to render before main camera
/// - **Initial State**: Inactive until snapshot requests arrive
///
/// # Texture Setup
/// Creates a temporary render texture with specific usage flags:
/// - **Format**: Matches FogMapSettings.snapshot_texture_format (typically RGBA8)
/// - **Dimensions**: texture_resolution_per_chunk from settings
/// - **Usage Flags**: RENDER_ATTACHMENT | TEXTURE_BINDING | COPY_DST | COPY_SRC
/// - **Asset Management**: Stored in SnapshotTempTexture resource
///
/// # Performance Characteristics
/// - **Memory**: One texture allocation matching chunk resolution
/// - **GPU Impact**: Creates render target and camera viewport
/// - **Time Complexity**: O(1) - fixed setup cost
/// - **Asset System**: Registers texture handle for lifetime management
///
/// # Resource Dependencies
/// - **FogMapSettings**: Provides texture dimensions and format configuration
/// - **`Assets<Image>`**: Manages temporary texture asset lifecycle
/// - **Commands**: Spawns camera entity with required components
///
/// # Coordinate System
/// The camera uses world coordinates with orthographic projection:
/// - Scale factor: chunk_size / texture_resolution (world units per pixel)
/// - Viewport: Matches texture dimensions exactly for 1:1 pixel mapping
fn setup_snapshot_camera(
    mut commands: Commands,
    settings: Res<FogMapSettings>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut snapshot_temp_image = Image::new_fill(
        Extent3d {
            width: settings.texture_resolution_per_chunk.x,
            height: settings.texture_resolution_per_chunk.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0; 4],
        settings.snapshot_texture_format,
        RenderAssetUsages::default(),
    );
    snapshot_temp_image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT // To render snapshots into / 用于渲染快照
        | TextureUsages::TEXTURE_BINDING // For sampling in overlay shader / 用于在覆盖 shader 中采样
        | TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
        | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输

    let snapshot_temp_handle = images.add(snapshot_temp_image);

    commands.insert_resource(SnapshotTempTexture {
        handle: snapshot_temp_handle.clone(),
    });
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: settings.chunk_size.x as f32 / settings.texture_resolution_per_chunk.x as f32,
            scaling_mode: bevy::render::camera::ScalingMode::Fixed {
                width: settings.texture_resolution_per_chunk.x as f32,
                height: settings.texture_resolution_per_chunk.y as f32,
            },
            ..OrthographicProjection::default_2d()
        }),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            order: -1,        // Render before the main camera, or as needed by graph
            is_active: false, // Initially inactive
            hdr: false,       // Snapshots likely don't need HDR
            target: RenderTarget::Image(snapshot_temp_handle.clone().into()),
            ..default()
        },
        SnapshotCamera, // Mark it as our snapshot camera
        SNAPSHOT_RENDER_LAYER,
    ));
}

/// Manages snapshot camera timing and deactivation after capture completion.
/// 管理快照相机计时并在捕获完成后停用相机
///
/// # Frame Timing Management
/// This system implements a frame counter mechanism to ensure proper snapshot capture:
/// 1. **Frame Countdown**: Decrements frame_to_wait counter each frame during capture
/// 2. **Completion Detection**: When counter reaches 0, snapshot is considered complete
/// 3. **Camera Deactivation**: Disables camera and resets state for next request
/// 4. **State Cleanup**: Clears capturing flag and layer index for next cycle
///
/// # Synchronization Logic
/// The timing ensures proper coordination between camera, render, and copy operations:
/// - **Frame 0**: Camera positioned and activated (prepare_snapshot_camera)
/// - **Frame 1**: Scene rendered to temporary texture by Bevy render system
/// - **Frame 2**: SnapshotNode copies texture data to final array layer
/// - **Frame 3**: Camera deactivated and ready for next request (this system)
///
/// # Performance Characteristics
/// - **CPU Cost**: Minimal - simple counter decrement and boolean updates
/// - **Memory Impact**: No allocations, only state field updates
/// - **Time Complexity**: O(1) per frame
/// - **GPU Coordination**: Prevents camera interference with subsequent operations
///
/// # State Transitions
/// ```text
/// capturing=false → [request arrives] → capturing=true, frame_to_wait=2
///       ↑                                           ↓
/// camera.is_active=false ← [this system] ← frame_to_wait=0
/// capturing=false
/// snapshot_layer_index=None
/// ```
fn check_snapshot_image_ready(
    mut snapshot_camera_state: ResMut<SnapshotCameraState>,
    snapshot_camera_query: Single<&mut Camera, With<SnapshotCamera>>,
) {
    if snapshot_camera_state.capturing {
        if snapshot_camera_state.frame_to_wait > 0 {
            snapshot_camera_state.frame_to_wait -= 1;
            return;
        }

        if snapshot_camera_state.frame_to_wait == 0 {
            let mut camera = snapshot_camera_query.into_inner();
            camera.is_active = false;
            snapshot_camera_state.snapshot_layer_index = None;
            snapshot_camera_state.capturing = false;
        }
    }
}

/// Resource to manage the state of the snapshot camera entity in the RenderWorld.
/// 用于管理 RenderWorld 中快照相机实体状态的资源。
///
/// This resource coordinates snapshot capture timing and state across main and render worlds.
/// It implements a frame-based synchronization mechanism to ensure proper snapshot capture
/// and texture copy operations.
///
/// # State Management Fields
/// - **capturing**: Indicates if camera is actively processing a snapshot request
/// - **frame_to_wait**: Countdown timer for proper render timing synchronization
/// - **snapshot_layer_index**: Target texture array layer for current snapshot
/// - **need_clear_layer_index**: Layer index that needs clearing (future use)
///
/// # Synchronization Protocol
/// ```text
/// Frame N:   prepare_snapshot_camera sets capturing=true, frame_to_wait=2
/// Frame N+1: Camera renders scene to temporary texture, frame_to_wait=1
/// Frame N+2: SnapshotNode copies texture to array layer, frame_to_wait=0
/// Frame N+3: check_snapshot_image_ready resets capturing=false
/// ```
///
/// # Cross-World Communication
/// This resource is automatically extracted from main world to render world via
/// ExtractResourcePlugin, enabling seamless state synchronization between ECS worlds.
///
/// # Performance Characteristics
/// - **Memory**: Minimal - 4 fields totaling ~12 bytes
/// - **Extract Cost**: O(1) per frame for cross-world sync
/// - **State Changes**: Infrequent, only during active snapshot operations
/// - **Lifetime**: Persistent resource, state changes per snapshot request
///
/// # Thread Safety
/// Safe for cross-world access via Bevy's extract system. The render world
/// gets a cloned copy each frame, preventing data races.
#[derive(Resource, ExtractResource, Clone, Default)]
pub struct SnapshotCameraState {
    /// Flag indicating whether camera is currently capturing a snapshot.
    /// 指示相机当前是否正在捕获快照的标志
    pub capturing: bool,
    /// Number of frames to wait before snapshot is considered complete.
    /// 快照被认为完成之前需要等待的帧数
    pub frame_to_wait: u8,
    /// Current target layer index in snapshot texture array (None if inactive).
    /// 快照纹理数组中的当前目标层索引（如果不活跃则为None）
    pub snapshot_layer_index: Option<u32>,
    /// Layer index that needs to be cleared (reserved for future use).
    /// 需要清除的层索引（保留供将来使用）
    pub need_clear_layer_index: Option<u32>,
}

/// Render graph label for the snapshot processing node.
/// 快照处理节点的渲染图标签
///
/// This label is used to identify and reference the SnapshotNode within Bevy's
/// render graph system. It enables proper ordering and dependency management
/// between render passes.
///
/// # Usage in Render Graph
/// ```rust,no_run
/// render_app.add_render_graph_edges(
///     Core2d,
///     (
///         Node2d::MainTransparentPass,
///         SnapshotNodeLabel,  // This label
///         FogComputeNodeLabel,
///         FogOverlayNodeLabel,
///         Node2d::EndMainPass,
///     ),
/// );
/// ```
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct SnapshotNodeLabel;

/// Render graph node that copies completed snapshot textures to the final texture array.
/// 将完成的快照纹理复制到最终纹理数组的渲染图节点
///
/// # Node Responsibilities
/// This render node executes during the render graph to transfer snapshot data:
/// 1. **Timing Verification**: Ensures snapshot capture timing is complete (frame_to_wait=0)
/// 2. **Resource Access**: Retrieves temporary and final snapshot texture handles
/// 3. **Texture Copy**: Performs GPU texture-to-texture copy operation
/// 4. **Layer Targeting**: Copies to specific array layer based on snapshot request
///
/// # GPU Operations
/// Executes a direct texture copy command on the GPU:
/// - **Source**: Temporary snapshot texture (2D render target)
/// - **Destination**: Final snapshot texture array (3D texture, specific layer)
/// - **Format**: Preserves original texture format and dimensions
/// - **Synchronization**: Executes within render graph command buffer
///
/// # Performance Characteristics
/// - **GPU Cost**: Single texture copy operation (very fast)
/// - **Memory**: No additional allocations during copy
/// - **Time Complexity**: O(1) per snapshot, O(texture_size) for copy
/// - **Bandwidth**: Limited by GPU memory bandwidth for texture transfer
///
/// # Integration with Render Graph
/// Positioned in render graph after MainTransparentPass to ensure scene is rendered:
/// ```text
/// MainTransparentPass → SnapshotNode → FogComputeNode → FogOverlayNode
/// ```
///
/// # Error Handling
/// The node gracefully handles missing resources by returning Ok(()) without operation.
/// This ensures render graph stability when snapshot system is inactive.
#[derive(Default)]
pub struct SnapshotNode;

impl Node for SnapshotNode {
    /// Executes the snapshot texture copy operation during render graph execution.
    /// 在渲染图执行期间执行快照纹理复制操作
    ///
    /// # Execution Logic
    /// 1. **View Validation**: Checks if current view entity has SnapshotCamera component
    /// 2. **State Verification**: Ensures camera state indicates ready snapshot (frame_to_wait=0)
    /// 3. **Resource Gathering**: Retrieves GPU image assets for source and destination textures
    /// 4. **Copy Operation**: Executes texture copy from temp texture to array layer
    ///
    /// # GPU Texture Copy Details
    /// - **Source**: snapshot_temp_image.texture (2D render target)
    /// - **Destination**: snapshot_images.texture (3D array, layer=layer_index)
    /// - **Extent**: Full texture dimensions (width x height x 1 layer)
    /// - **Origin**: (0,0,layer_index) for proper array layer targeting
    ///
    /// # Performance Impact
    /// - **GPU Time**: ~0.1ms for typical 256x256 RGBA8 texture copy
    /// - **Memory Bandwidth**: texture_width × texture_height × bytes_per_pixel
    /// - **Synchronization**: Automatically handled by render graph command buffer
    ///
    /// # Error Conditions
    /// Returns Ok(()) in all cases to maintain render graph stability:
    /// - Missing SnapshotCamera component (not our view)
    /// - Camera not ready (frame_to_wait > 0)
    /// - Missing texture resources (GPU assets not loaded)
    /// - No active snapshot layer (snapshot_layer_index = None)
    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let view_entity = graph.view_entity();

        if world.get::<SnapshotCamera>(view_entity).is_none() {
            return Ok(());
        }

        let camera_state = world.resource::<SnapshotCameraState>();
        if let Some(layer_index) = camera_state.snapshot_layer_index {
            if camera_state.frame_to_wait > 0 {
                return Ok(());
            }
            let gpu_images = world.resource::<RenderAssets<GpuImage>>();
            let render_snapshot_temp_texture = world.resource::<RenderSnapshotTempTexture>();
            let render_snapshot_texture = world.resource::<RenderSnapshotTexture>();

            let Some(snapshot_temp_image) = gpu_images.get(&render_snapshot_temp_texture.0) else {
                return Ok(());
            };

            let Some(snapshot_images) = gpu_images.get(&render_snapshot_texture.0) else {
                return Ok(());
            };

            render_context.command_encoder().copy_texture_to_texture(
                snapshot_temp_image.texture.as_image_copy(),
                TexelCopyTextureInfo {
                    texture: &snapshot_images.texture,
                    mip_level: 0,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: layer_index,
                    },
                    aspect: TextureAspect::All,
                },
                Extent3d {
                    width: snapshot_temp_image.size.width,
                    height: snapshot_temp_image.size.height,
                    depth_or_array_layers: 1,
                },
            );

            trace!(
                "Copying temp texture to snapshot layer {}. Temp texture size: {}x{}, format: {:?}",
                layer_index,
                snapshot_temp_image.size.width,
                snapshot_temp_image.size.height,
                snapshot_temp_image.texture_format
            );
        }

        Ok(())
    }
}

/// Ensures all Capturable entities are visible on the snapshot render layer.
/// 确保所有可捕获实体在快照渲染层上可见
///
/// # System Purpose
/// This system automatically manages render layer membership for entities that should
/// appear in snapshots. It ensures that any entity marked with Capturable component
/// is visible to the snapshot camera by adding them to the SNAPSHOT_RENDER_LAYER.
///
/// # Layer Management Strategy
/// - **Union Operation**: Combines existing render layers with snapshot layer
/// - **Default Handling**: Entities without RenderLayers get layer 0 + snapshot layer
/// - **Preservation**: Maintains all existing layer memberships
/// - **Automatic**: Runs every frame to catch newly spawned Capturable entities
///
/// # Performance Characteristics
/// - **Query Cost**: O(n) where n = number of Capturable entities
/// - **Update Cost**: O(m) where m = entities needing layer updates
/// - **Memory**: Minimal - only RenderLayers component updates
/// - **Time Complexity**: O(n) per frame for entity iteration
///
/// # Use Cases
/// - **Dynamic Entities**: Newly spawned entities automatically get correct layers
/// - **Layer Combinations**: Entities can exist on multiple render layers simultaneously
/// - **Snapshot Visibility**: Ensures all capturable content appears in snapshots
/// - **Maintenance-Free**: No manual layer management required for Capturable entities
///
/// # Integration Points
/// - **Capturable Component**: Queries entities marked for snapshot inclusion
/// - **RenderLayers System**: Integrates with Bevy's render layer filtering
/// - **Snapshot Camera**: Ensures camera can see all relevant entities
/// - **Render Graph**: Supports proper visibility culling in snapshot passes
pub fn ensure_snapshot_render_layer(
    mut commands: Commands,
    snapshot_visible_query: Query<(Entity, Option<&RenderLayers>), With<Capturable>>,
) {
    for (entity, existing_layers) in snapshot_visible_query.iter() {
        let snapshot_layer = SNAPSHOT_RENDER_LAYER.clone();
        let combined_layers = match existing_layers {
            Some(layers) => layers.union(&snapshot_layer),
            None => snapshot_layer.with(0),
        };

        commands.entity(entity).insert((
            combined_layers, // Ensure it's on the snapshot layer
        ));
    }
}

/// System to handle `RequestChunkSnapshot` and queue snapshot remakes.
/// 处理 `RequestChunkSnapshot` 事件并对快照重制进行排队的系统。
///
/// # Event Processing Logic
/// This system processes RequestChunkSnapshot events and converts them into snapshot creation requests:
/// 1. **Event Iteration**: Processes all RequestChunkSnapshot events from the frame
/// 2. **Chunk Validation**: Verifies chunk entity exists and has valid FogChunk component
/// 3. **Layer Verification**: Ensures chunk has allocated snapshot_layer_index
/// 4. **Deduplication**: Prevents duplicate requests for the same chunk coordinates
/// 5. **Queue Management**: Adds valid requests to MainWorldSnapshotRequestQueue
///
/// # Validation Checks
/// - **Entity Existence**: Chunk coordinates must map to valid entity in ChunkEntityManager
/// - **Component Validity**: Entity must have FogChunk component with proper data
/// - **Layer Allocation**: Chunk must have allocated snapshot_layer_index (not None)
/// - **Request Deduplication**: Prevents multiple pending requests for same coordinates
///
/// # Performance Characteristics
/// - **Event Processing**: O(e) where e = number of RequestChunkSnapshot events
/// - **Chunk Lookup**: O(1) hash map lookup per event
/// - **Deduplication**: O(r) where r = number of pending requests (typically small)
/// - **Memory**: Minimal allocations only for new queue entries
///
/// # Error Handling
/// Uses warn! logging for validation failures to aid debugging:
/// - Missing chunk entity in manager
/// - Failed FogChunk component query
/// - Missing snapshot_layer_index allocation
///
/// # System Ordering Requirements
/// - **After**: FogSystems::UpdateChunkState (chunk states updated)
/// - **Before**: FogSystems::ManageEntities (entities managed based on requests)
fn handle_request_chunk_snapshot_events(
    mut events: EventReader<RequestChunkSnapshot>,
    chunk_manager: Res<ChunkEntityManager>,
    chunk_query: Query<&FogChunk>, // Query for FogChunk to get its details / 查询 FogChunk 以获取其详细信息
    mut snapshot_requests: ResMut<MainWorldSnapshotRequestQueue>,
) {
    for event in events.read() {
        let chunk_coords = event.0;
        if let Some(entity) = chunk_manager.map.get(&chunk_coords) {
            if let Ok(chunk) = chunk_query.get(*entity) {
                if let Some(snapshot_layer_index) = chunk.snapshot_layer_index {
                    // Check if a snapshot for this chunk is already pending
                    // 检查此区块的快照是否已在等待队列中
                    let already_pending = snapshot_requests
                        .requests
                        .iter()
                        .any(|req| req.chunk_coords == chunk_coords);

                    if !already_pending {
                        trace!(
                            "Received RequestChunkSnapshot for {:?}. Queuing snapshot remake for layer {}.",
                            chunk_coords, snapshot_layer_index
                        );
                        snapshot_requests.requests.push(MainWorldSnapshotRequest {
                            chunk_coords,
                            snapshot_layer_index,
                            world_bounds: chunk.world_bounds,
                        });
                    } else {
                        debug!(
                            "Received RequestChunkSnapshot for {:?}, but snapshot remake is already pending. Skipping.",
                            chunk_coords
                        );
                    }
                } else {
                    warn!(
                        "Received RequestChunkSnapshot for {:?}, but chunk has no snapshot_layer_index. Cannot request snapshot.",
                        chunk_coords
                    );
                }
            } else {
                warn!(
                    "Received RequestChunkSnapshot for {:?}, but failed to get FogChunk component.",
                    chunk_coords
                );
            }
        } else {
            warn!(
                "Received RequestChunkSnapshot for {:?}, but chunk entity not found in manager.",
                chunk_coords
            );
        }
    }
}
