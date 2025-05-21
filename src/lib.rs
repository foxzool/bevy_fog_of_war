use self::prelude::*;
use crate::render::FogOfWarRenderPlugin;
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageSampler, ImageSamplerDescriptor},
    platform::collections::HashSet,
    render::{
        camera::RenderTarget, extract_component::ExtractComponentPlugin,
        extract_resource::ExtractResourcePlugin, render_resource::Extent3d,
        render_resource::TextureDimension, render_resource::TextureUsages,
    },
};

mod components;
pub mod prelude;
mod render;
mod resources;
mod snapshot;

/// Event to request a snapshot for a specific chunk.
/// 请求为特定区块生成快照的事件。
#[derive(Event, Debug, Clone, Copy)]
pub struct RequestChunkSnapshotEvent(pub IVec2);

/// Component to define the bounding box of a capturable entity.
/// 用于定义可捕获实体边界框的组件。
#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component)]
pub struct CapturableBounds {
    /// Half-extents of the bounding box relative to the entity's transform.
    /// 相对于实体变换的边界框半区。
    pub half_extents: Vec2,
}

impl Default for CapturableBounds {
    fn default() -> Self {
        // Default to a small size, e.g., 16x16. Adjust as needed.
        // 默认为一个小尺寸，例如 16x16。根据需要调整。
        Self {
            half_extents: Vec2::splat(8.0),
        }
    }
}

/// Component to store the last known chunk coordinates occupied by a capturable entity.
/// 用于存储可捕获实体最后占据的区块坐标的组件。
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
struct LastKnownOccupiedChunks(Vec<IVec2>);

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
enum FogSystemSet {
    /// Update chunk states based on vision and camera
    /// 更新区块状态 (基于视野和相机)
    UpdateChunkState,
    /// Manage chunk entities (creation, activation)
    /// 管理区块实体 (创建, 激活)
    ManageEntities,
    /// Handle CPU <-> GPU memory transfer logic
    /// 处理 CPU <-> GPU 内存传输逻辑
    PrepareTransfers,
}

pub struct FogOfWarPlugin;

impl Plugin for FogOfWarPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<VisionSource>()
            .register_type::<FogChunk>()
            .register_type::<Capturable>()
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
            .init_resource::<MainWorldSnapshotRequestQueue>();

        app.add_event::<ChunkGpuDataReadyEvent>()
            .add_event::<ChunkCpuDataUploadedEvent>()
            .add_event::<RequestChunkSnapshotEvent>(); // Added event for remaking snapshots / 添加用于重制快照的事件

        app.add_plugins(ExtractResourcePlugin::<GpuToCpuCopyRequests>::default())
            .add_plugins(ExtractResourcePlugin::<CpuToGpuCopyRequests>::default())
            .add_plugins(ExtractComponentPlugin::<SnapshotCamera>::default());

        app.configure_sets(
            Update,
            (
                FogSystemSet::UpdateChunkState,
                FogSystemSet::ManageEntities,
                FogSystemSet::PrepareTransfers,
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
                .in_set(FogSystemSet::UpdateChunkState),
        );

        app.add_systems(
            Update,
            (manage_chunk_entities).in_set(FogSystemSet::ManageEntities),
        );

        app.add_systems(
            Update,
            manage_chunk_texture_transfer.in_set(FogSystemSet::PrepareTransfers),
        );

        app.add_plugins(FogOfWarRenderPlugin);
        app.add_plugins(snapshot::SnapshotPlugin);
    }
}

fn auto_add_capturable_bounds_from_sprite(
    mut commands: Commands,
    query: Query<(Entity, &Sprite), (With<Capturable>, Without<CapturableBounds>)>,
    images: Res<Assets<Image>>,
    texture_atlas_layouts: Res<Assets<TextureAtlasLayout>>,
) {
    for (entity, sprite) in query.iter() {
        let mut entity_size: Option<Vec2> = None;

        if let Some(custom_size) = sprite.custom_size {
            entity_size = Some(custom_size);
        } else if let Some(image) = images.get(&sprite.image) {
            entity_size = Some(image.size_f32());
        }

        if entity_size.is_none() {
            if let Some(atlas_sprite) = &sprite.texture_atlas {
                if let Some(layout) = texture_atlas_layouts.get(&atlas_sprite.layout) {
                    if let Some(rect) = layout.textures.get(atlas_sprite.index) {
                        entity_size = Some(rect.size().as_vec2());
                    }
                }
            }
        }

        if let Some(size) = entity_size {
            if size.x > 0.0 && size.y > 0.0 {
                let half_extents = size * 1.2 / 2.0;
                commands
                    .entity(entity)
                    .insert(CapturableBounds { half_extents });
                info!(
                    "Automatically added CapturableBounds {{ half_extents: {:?} }} to entity {:?} based on sprite/atlas size {:?}",
                    half_extents, entity, size
                );
            } else {
                warn!(
                    "Entity {:?} has Capturable but its sprite/atlas size is zero or negative: {:?}. CapturableBounds not added.",
                    entity, size
                );
            }
        } else {
            warn!(
                "Entity {:?} has Capturable but no Sprite/TextureAtlas or size could not be determined. CapturableBounds not added.",
                entity
            );
        }
    }
}

fn setup_fog_resources(
    mut commands: Commands,
    settings: Res<FogMapSettings>,
    mut images: ResMut<Assets<Image>>,
) {
    // --- Create Texture Arrays ---
    // --- 创建 Texture Arrays ---
    info!("Setting up Fog of War with {} layers.", settings.max_layers);

    let fog_texture_size = Extent3d {
        width: settings.texture_resolution_per_chunk.x,
        height: settings.texture_resolution_per_chunk.y,
        depth_or_array_layers: settings.max_layers,
    };
    let snapshot_texture_size = fog_texture_size;
    let visibility_texture_size = fog_texture_size;

    // Fog Texture: R8Unorm (0=visible, 1=unexplored)
    // 雾效纹理: R8Unorm (0=可见, 1=未探索)
    let fog_initial_data = vec![
        0u8;
        (fog_texture_size.width * fog_texture_size.height * fog_texture_size.depth_or_array_layers)
            as usize
    ];
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

    let visibility_initial_data = vec![
        0u8;
        (visibility_texture_size.width
            * visibility_texture_size.height
            * visibility_texture_size.depth_or_array_layers)
            as usize
    ];
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
    let snapshot_initial_data = vec![
        0u8;
        (snapshot_texture_size.width
            * snapshot_texture_size.height
            * snapshot_texture_size.depth_or_array_layers
            * 4) as usize
    ]; // 4 bytes per pixel for RGBA / RGBA 每像素 4 字节
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
    commands.insert_resource(TextureArrayManager::new(settings.max_layers));

    info!("Fog of War resources initialized, including SnapshotCamera.");
}

/// Clears caches that are rebuilt each frame.
/// 清除每帧重建的缓存。
fn clear_per_frame_caches(mut cache: ResMut<ChunkStateCache>) {
    cache.visible_chunks.clear();
    cache.camera_view_chunks.clear();
}

/// Updates visible and explored chunk sets based on VisionSource positions.
/// 根据 VisionSource 位置更新可见和已探索的区块集合。
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
    mut snapshot_event_writer: EventWriter<RequestChunkSnapshotEvent>, // Changed to EventWriter / 更改为 EventWriter
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
                        info!(
                            "Chunk {:?} ({}) {} ({} -> {}). Sending RequestChunkSnapshotEvent.",
                            *coords,
                            entity.index(),
                            reason,
                            old_visibility,
                            new_visibility
                        );
                        snapshot_event_writer.write(RequestChunkSnapshotEvent(*coords));
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
                }
                // Ensure it's marked as GPU resident in cache (will be done in memory logic)
                // 确保在缓存中标记为 GPU 驻留 (将在内存逻辑中完成)
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

/// Check if a circle intersects with a rectangle
/// 检查圆形是否与矩形相交
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

/// 管理区块纹理数据在 CPU 和 GPU 之间的传输。
/// Manages the transfer of chunk texture data between CPU and GPU.
#[allow(clippy::too_many_arguments)]
pub fn manage_chunk_texture_transfer(
    mut commands: Commands,
    mut chunk_query: Query<(Entity, &mut FogChunk, &mut FogChunkImage)>,
    chunk_cache: Res<ChunkStateCache>,
    mut images: ResMut<Assets<Image>>,
    mut texture_manager: ResMut<TextureArrayManager>,
    mut gpu_to_cpu_requests: ResMut<GpuToCpuCopyRequests>,
    mut cpu_to_gpu_requests: ResMut<CpuToGpuCopyRequests>,
    mut gpu_data_ready_reader: EventReader<ChunkGpuDataReadyEvent>,
    mut cpu_data_uploaded_reader: EventReader<ChunkCpuDataUploadedEvent>,
    mut snapshot_requests: ResMut<MainWorldSnapshotRequestQueue>,
) {
    for event in gpu_data_ready_reader.read() {
        if let Some((_entity, mut chunk, chunk_image)) = chunk_query
            .iter_mut()
            .find(|(_, c, _)| c.coords == event.chunk_coords)
        {
            if chunk.state.memory_location == ChunkMemoryLocation::PendingCopyToCpu {
                info!(
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
                    "Chunk {:?}: Received GpuDataReadyEvent but state is {:?}, expected PendingCopyToCpu.",
                    event.chunk_coords, chunk.state.memory_location
                );
            }
        } else {
            warn!(
                "Received GpuDataReadyEvent for unknown chunk: {:?}",
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
                info!(
                    "Chunk {:?}: CPU->GPU upload complete. Now resident on GPU. Layers F{}, S{}.",
                    event.chunk_coords,
                    chunk.fog_layer_index.unwrap(),
                    chunk.snapshot_layer_index.unwrap()
                );
                chunk.state.memory_location = ChunkMemoryLocation::Gpu;
            } else {
                warn!(
                    "Chunk {:?}: Received CpuDataUploadedEvent but state is {:?}, expected PendingCopyToGpu.",
                    event.chunk_coords, chunk.state.memory_location
                );
            }
        } else {
            warn!(
                "Received CpuDataUploadedEvent for unknown chunk: {:?}",
                event.chunk_coords
            );
        }
    }

    // 清空本帧的请求队列，因为它们将被重新评估
    // Clear this frame's request queues as they will be re-evaluated
    gpu_to_cpu_requests.requests.clear();
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
    // 你可能还想为 target_gpu_chunks 周围添加一个缓冲区
    // You might also want to add a buffer zone around target_gpu_chunks

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
                        info!(
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
                        // 索引在 GpuDataReadyEvent 事件处理中设为 None
                        // Indices are set to None in GpuDataReadyEvent event handling
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
                    info!(
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
                    if let Some((fog_idx_val, snap_idx_val)) =
                        texture_manager.allocate_layer_indices(chunk.coords)
                    {
                        info!(
                            "Chunk {:?}: Requesting CPU -> GPU transfer. Layers: F{}, S{}",
                            chunk.coords, fog_idx_val, snap_idx_val
                        );
                        chunk.fog_layer_index = Some(fog_idx_val);
                        chunk.snapshot_layer_index = Some(snap_idx_val);
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
                        // (可选：可以等到 ChunkCpuDataUploadedEvent 再移除，以防上传失败)
                        // (Optional: can wait for ChunkCpuDataUploadedEvent before removing, in case upload fails)
                        // cpu_storage.storage.remove(&chunk.coords);
                    } else {
                        warn!(
                            "Chunk {:?}: Wanted to move CPU -> GPU, but no free texture layers!",
                            chunk.coords
                        );
                    }
                }
            }
            ChunkMemoryLocation::PendingCopyToCpu | ChunkMemoryLocation::PendingCopyToGpu => {
                // 正在传输中，等待事件
                // In transit, waiting for event
            }
        }
    }
}

/// System to detect movement of Capturable entities and request snapshot remakes for affected chunks.
/// It considers the bounds of the capturable, potentially affecting multiple chunks.
/// 检测 Capturable 实体移动并为受影响区块请求快照重制的系统。
/// 它会考虑可捕获对象的边界，可能影响多个区块。
fn trigger_snapshot_remake_on_capturable_move_multi_chunk(
    mut query: Query<
        (
            Entity,
            &GlobalTransform,
            &CapturableBounds,
            Option<&mut LastKnownOccupiedChunks>,
        ),
        (With<Capturable>, Changed<GlobalTransform>),
    >,
    settings: Res<FogMapSettings>,
    mut snapshot_event_writer: EventWriter<RequestChunkSnapshotEvent>,
    mut commands: Commands,
) {
    for (entity, global_transform, bounds, mut last_known_chunks_opt) in query.iter_mut() {
        let current_center_pos = global_transform.translation().truncate();

        // Calculate AABB (Axis-Aligned Bounding Box) in world space
        // 计算世界空间中的 AABB（轴对齐边界框）
        let min_world = current_center_pos - bounds.half_extents;
        let max_world = current_center_pos + bounds.half_extents;

        // Convert AABB to min/max chunk coordinates
        // 将 AABB 转换为最小/最大区块坐标
        let min_chunk_coords = settings.world_to_chunk_coords(min_world);
        let max_chunk_coords = settings.world_to_chunk_coords(max_world);

        let mut current_occupied_chunks = HashSet::new();
        for x in min_chunk_coords.x..=max_chunk_coords.x {
            for y in min_chunk_coords.y..=max_chunk_coords.y {
                current_occupied_chunks.insert(IVec2::new(x, y));
            }
        }

        let mut chunks_to_update = HashSet::new();

        if let Some(ref mut last_known) = last_known_chunks_opt {
            let last_occupied_set: HashSet<IVec2> = last_known.0.iter().cloned().collect();

            // Chunks the entity entered (present in current, not in last)
            // 实体进入的区块 (存在于当前，但不存在于上一次)
            for &new_chunk in current_occupied_chunks.difference(&last_occupied_set) {
                chunks_to_update.insert(new_chunk);
            }
            // Chunks the entity exited (present in last, not in current)
            // 实体离开的区块 (存在于上一次，但不存在于当前)
            for &old_chunk in last_occupied_set.difference(&current_occupied_chunks) {
                chunks_to_update.insert(old_chunk);
            }

            // Update the component with the new set of occupied chunks
            // 使用新的已占据区块集合更新组件
            last_known.0 = current_occupied_chunks.iter().cloned().collect();
        } else {
            // This is the first time we've processed this entity after a move (or it was just added with Changed<GlobalTransform>)
            // 这是我们第一次处理该实体的移动（或者它刚刚被添加并带有 Changed<GlobalTransform>）
            // or the LastKnownOccupiedChunks component was missing.
            // 或者 LastKnownOccupiedChunks 组件缺失。
            // Consider all currently occupied chunks as needing an update and add the component.
            // 将所有当前占据的区块视为需要更新，并添加组件。
            for &chunk in current_occupied_chunks.iter() {
                chunks_to_update.insert(chunk);
            }
            commands.entity(entity).insert(LastKnownOccupiedChunks(
                current_occupied_chunks.iter().cloned().collect(),
            ));
        }

        if !chunks_to_update.is_empty() {
            // Log specific chunks being updated due to movement / 记录由于移动而更新的特定区块
            for coords_to_update_single in chunks_to_update.iter() {
                info!(
                    "Capturable entity {:?} movement. Requesting snapshot for chunk: {:?}. Center: {:?}, Bounds: {:?}, Min/Max Chunks: ({:?},{:?})",
                    entity,
                    coords_to_update_single,
                    current_center_pos,
                    bounds.half_extents,
                    min_chunk_coords,
                    max_chunk_coords
                );
            }
            for coords in chunks_to_update {
                snapshot_event_writer.write(RequestChunkSnapshotEvent(coords));
            }
        }
    }
}
