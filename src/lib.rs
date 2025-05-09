use self::prelude::*;
use crate::render::FogOfWarRenderPlugin;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::platform::collections::HashSet;
use bevy::render::camera::RenderTarget;
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureUsages};

mod components;
pub mod prelude;
mod render;
mod resources;

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
            .register_type::<Snapshottable>()
            .register_type::<ChunkVisibility>()
            .register_type::<ChunkMemoryLocation>()
            .register_type::<ChunkState>()
            // .register_type::<FogMapSettings>()
            .register_type::<FogTextureArray>()
            .register_type::<SnapshotTextureArray>()
            .register_type::<ChunkEntityManager>()
            .register_type::<ChunkStateCache>()
            .register_type::<TextureArrayManager>()
            .register_type::<CpuChunkStorage>()
            .register_type::<GpuToCpuCopyRequests>()
            .register_type::<CpuToGpuCopyRequests>()
            .register_type::<MainWorldSnapshotRequestQueue>();

        app.init_resource::<FogMapSettings>()
            .init_resource::<ChunkEntityManager>()
            .init_resource::<ChunkStateCache>()
            .init_resource::<GpuToCpuCopyRequests>()
            .init_resource::<CpuToGpuCopyRequests>()
            .init_resource::<MainWorldSnapshotRequestQueue>()
            .init_resource::<CpuChunkStorage>();

        app.add_event::<ChunkGpuDataReadyEvent>()
            .add_event::<ChunkCpuDataUploadedEvent>();

        app.add_plugins(ExtractResourcePlugin::<GpuToCpuCopyRequests>::default())
            .add_plugins(ExtractResourcePlugin::<CpuToGpuCopyRequests>::default());

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
            manage_chunk_entities.in_set(FogSystemSet::ManageEntities),
        );

        app.add_systems(
            Update,
            manage_chunk_texture_transfer.in_set(FogSystemSet::PrepareTransfers),
        );

        app.add_plugins(FogOfWarRenderPlugin);

        // app.add_plugins(ChunkManagerPlugin)
        //     .add_plugins(vision::VisionComputePlugin)
        //     .add_plugins(fog_2d::Fog2DRenderPlugin)
        //     .add_plugins(sync_texture::GpuSyncTexturePlugin);
    }
}

fn setup_fog_resources(
    mut commands: Commands,
    settings: Res<FogMapSettings>,
    mut images: ResMut<Assets<Image>>,
) {
    // --- Create Texture Arrays ---
    // --- 创建 Texture Arrays ---
    let array_layers = 64; // Example layer count, adjust as needed / 示例层数，按需调整
    info!("Setting up Fog of War with {} layers.", array_layers);

    let fog_texture_size = Extent3d {
        width: settings.texture_resolution_per_chunk.x,
        height: settings.texture_resolution_per_chunk.y,
        depth_or_array_layers: array_layers,
    };
    let snapshot_texture_size = fog_texture_size;

    // Fog Texture: R8Unorm (0=visible, 1=unexplored)
    // 雾效纹理: R8Unorm (0=可见, 1=未探索)
    let fog_initial_data = vec![
        255u8;
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
        RenderAssetUsages::RENDER_WORLD,
    );
    snapshot_image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT // To render snapshots into / 用于渲染快照
        | TextureUsages::TEXTURE_BINDING // For sampling in overlay shader / 用于在覆盖 shader 中采样
        | TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
        | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输

    let fog_handle = images.add(fog_image);
    let snapshot_handle = images.add(snapshot_image);

    // Insert resources
    // 插入资源
    commands.insert_resource(FogTextureArray { handle: fog_handle });
    commands.insert_resource(SnapshotTextureArray {
        handle: snapshot_handle,
    });
    commands.insert_resource(TextureArrayManager::new(array_layers));

    info!("Fog of War resources initialized.");
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

            if chunk.state.visibility != new_visibility {
                // info!("Chunk {:?} visibility changed to {:?}", coords, new_visibility);
                chunk.state.visibility = new_visibility;
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
    cpu_storage: Res<CpuChunkStorage>,
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

                // Check if data exists in CPU storage (was previously offloaded)
                // 检查 CPU 存储中是否存在数据 (之前被卸载过)
                let initial_state = if cpu_storage.storage.contains_key(&coords) {
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
                    .spawn(FogChunk {
                        coords,
                        fog_layer_index: Some(fog_idx),
                        snapshot_layer_index: Some(snap_idx),
                        state: initial_state,
                        world_bounds,
                    })
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
    mut chunk_query: Query<(Entity, &mut FogChunk)>,
    chunk_cache: Res<ChunkStateCache>,
    mut texture_manager: ResMut<TextureArrayManager>,
    mut cpu_storage: ResMut<CpuChunkStorage>,
    mut gpu_to_cpu_requests: ResMut<GpuToCpuCopyRequests>,
    mut cpu_to_gpu_requests: ResMut<CpuToGpuCopyRequests>,
    mut gpu_data_ready_reader: EventReader<ChunkGpuDataReadyEvent>,
    mut cpu_data_uploaded_reader: EventReader<ChunkCpuDataUploadedEvent>,
) {
    // --- 1. 处理来自 RenderApp 的事件 ---
    // --- 1. Process events from RenderApp ---

    for event in gpu_data_ready_reader.read() {
        if let Some((_entity, mut chunk)) = chunk_query
            .iter_mut()
            .find(|(_, c)| c.coords == event.chunk_coords)
        {
            if chunk.state.memory_location == ChunkMemoryLocation::PendingCopyToCpu {
                info!(
                    "Chunk {:?}: GPU->CPU copy complete. Storing in CPU. Layers F{}, S{}",
                    event.chunk_coords,
                    chunk.fog_layer_index.unwrap(),
                    chunk.snapshot_layer_index.unwrap()
                );
                cpu_storage.storage.insert(
                    event.chunk_coords,
                    (event.fog_data.clone(), event.snapshot_data.clone()),
                );

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
        if let Some((_entity, mut chunk)) = chunk_query
            .iter_mut()
            .find(|(_, c)| c.coords == event.chunk_coords)
        {
            if chunk.state.memory_location == ChunkMemoryLocation::PendingCopyToGpu {
                info!(
                    "Chunk {:?}: CPU->GPU upload complete. Now resident on GPU. Layers F{}, S{}.",
                    event.chunk_coords,
                    chunk.fog_layer_index.unwrap(),
                    chunk.snapshot_layer_index.unwrap()
                );
                chunk.state.memory_location = ChunkMemoryLocation::Gpu;
                cpu_storage.storage.remove(&chunk.coords);
                // chunk_cache.gpu_resident_chunks is typically updated by manage_chunk_entities
                // or a dedicated system that reacts to FogChunk state changes.
                // Let's assume another system handles adding to gpu_resident_chunks in the cache
                // based on the Gpu state. Or, we could do it here if careful about consistency.
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
    for (entity, mut chunk) in chunk_query.iter_mut() {
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
                    if let Some((fog_data, snapshot_data)) = cpu_storage.storage.get(&chunk.coords)
                    {
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
                                fog_data: fog_data.clone(), // TODO: Avoid clone if possible, maybe Rc or Arc? Or transfer ownership.
                                snapshot_data: snapshot_data.clone(),
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
                    } else {
                        // 在 CPU 存储中没有数据，但被认为是 Cpu 状态。这可能是一个错误，
                        // 或者它是一个新创建的区块，其初始状态被设为 Cpu，并且应该通过其他逻辑变为 Unexplored/Gpu。
                        // No data in CPU storage, but considered Cpu state. This might be an error,
                        // or it's a newly created chunk whose initial state was set to Cpu and should become Unexplored/Gpu via other logic.
                        // 这种情况更可能由 manage_chunk_entities 处理：如果区块需要且不在CPU，则作为新区块创建在GPU。
                        // This scenario is more likely handled by manage_chunk_entities: if chunk is needed and not on CPU, create as new on GPU.
                        warn!(
                            "Chunk {:?}: Is Cpu state but no data in CpuChunkStorage. This might be an issue or needs creation logic.",
                            chunk.coords
                        );
                        // 可能是 manage_chunk_entities 应该创建一个全新的区块在 GPU 上
                        // Potentially, manage_chunk_entities should create a brand new chunk on GPU.
                        // For now, we'll assume manage_chunk_entities handles new chunk creation on GPU if no CPU data exists.
                    }
                }
            }
            ChunkMemoryLocation::PendingCopyToCpu | ChunkMemoryLocation::PendingCopyToGpu => {
                // 正在传输中，等待事件
                // In transit, waiting for event
            }
        }
    }
    // TODO: 更新 ChunkStateCache.gpu_resident_chunks
    //       这可以在一个单独的系统或这里完成，通过迭代所有 chunk_query 并检查其最终的 memory_location
    // TODO: Update ChunkStateCache.gpu_resident_chunks
    //       This can be done in a separate system or here by iterating all chunk_query and checking their final memory_location
}
