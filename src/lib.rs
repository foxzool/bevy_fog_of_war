use self::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::platform::collections::HashSet;
use bevy::render::camera::RenderTarget;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureUsages};

mod chunk;
mod components;
mod fog_2d;
pub mod prelude;
mod resources;
mod sync_texture;
mod vision;

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
    ManageMemory,
    /// Prepare data for GPU processing (runs before Render Graph execution)
    /// 为 GPU 处理准备数据 (在 Render Graph 执行前运行)
    PrepareGpuData,
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
            .register_type::<CpuChunkStorage>();

        app.init_resource::<FogMapSettings>()
            .init_resource::<ChunkEntityManager>()
            .init_resource::<ChunkStateCache>()
            .init_resource::<CpuChunkStorage>();


        app.configure_sets(
            Update,
            (
                FogSystemSet::UpdateChunkState,
                FogSystemSet::ManageEntities,
                FogSystemSet::ManageMemory,
            )
                .chain(), // Ensure they run in this order / 确保它们按此顺序运行
        );


        app.add_systems(Startup, setup_fog_resources);

        app.add_systems(
            Update,
            (
                // --- UpdateChunkState Set ---
                clear_per_frame_caches, // Run first in the set / 在集合中首先运行
                update_chunk_visibility,
                update_camera_view_chunks,
                update_chunk_component_state, // Sync cache state to components / 将缓存状态同步到组件
            )
                .in_set(FogSystemSet::UpdateChunkState),
        );

        app.add_systems(
            Update,
            manage_chunk_entities.in_set(FogSystemSet::ManageEntities),
        );

        app.add_plugins(ChunkManagerPlugin)
            .add_plugins(vision::VisionComputePlugin)
            .add_plugins(fog_2d::Fog2DRenderPlugin)
            .add_plugins(sync_texture::GpuSyncTexturePlugin);
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
    // explored_chunks persists / explored_chunks 会持久存在
    // gpu_resident_chunks is managed by memory system / gpu_resident_chunks 由内存系统管理
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
                let chunk_center_world = (chunk_coords.as_vec2() + 0.5) * chunk_size;

                // Simple distance check (center to source) - more accurate checks possible
                // 简单的距离检查 (区块中心到源点) - 可以进行更精确的检查
                if chunk_center_world.distance_squared(source_pos) <= range_sq {
                    // Mark as visible and explored in the cache
                    // 在缓存中标记为可见和已探索
                    cache.visible_chunks.insert(chunk_coords);
                    cache.explored_chunks.insert(chunk_coords);
                }
                // Alternative: Check if circle intersects chunk rect
                // 备选方案: 检查圆形是否与区块矩形相交
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
    mut cpu_storage: ResMut<CpuChunkStorage>,
    mut chunk_q: Query<&mut FogChunk>, // Query to update state if chunk already exists
                                       // We might need Assets<Image> here if we immediately upload from CPU storage
                                       // 如果我们立即从 CPU 存储上传，这里可能需要 Assets<Image>
) {
    let chunk_size_f = settings.chunk_size.as_vec2();
    let chunk_size_i = settings.chunk_size.as_ivec2();

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
            if let Ok(mut chunk) = chunk_q.get_mut(*entity) {
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
                    // Brand new chunk, starts Unexplored on GPU
                    // 全新区块，在 GPU 上以 Unexplored 开始
                    ChunkState {
                        visibility: ChunkVisibility::Unexplored, // Visibility updated later / 可见性稍后更新
                        memory_location: ChunkMemoryLocation::Gpu,
                    }
                };

                let entity = commands
                    .spawn(FogChunk {
                        coords,
                        layer_index: None,
                        screen_index: None,
                        fog_layer_index: fog_idx,
                        snapshot_layer_index: snap_idx,
                        loaded: false,
                        state: initial_state,
                        world_bounds,
                    })
                    .id();

                chunk_manager.map.insert(coords, entity);
                if initial_state.memory_location == ChunkMemoryLocation::Gpu {
                    cache.gpu_resident_chunks.insert(coords); // Mark as GPU resident / 标记为 GPU 驻留
                }
                // info!("Created FogChunk {:?} (Fog: {}, Snap: {}) State: {:?}", coords, fog_idx, snap_idx, initial_state);
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

    // Update state for chunks transitioning from CPU to GPU
    // 更新从 CPU 转换到 GPU 的区块状态
    for coords in chunks_to_make_gpu {
        if let Some(entity) = chunk_manager.map.get(&coords) {
            if let Ok(mut chunk) = chunk_q.get_mut(*entity) {
                chunk.state.memory_location = ChunkMemoryLocation::Gpu;
                cache.gpu_resident_chunks.insert(coords); // Mark as GPU resident / 标记为 GPU 驻留
                // Remove from CPU storage (data transfer happens elsewhere)
                // 从 CPU 存储中移除 (数据传输在别处发生)
                cpu_storage.storage.remove(&coords);
                // info!("Chunk {:?} marked for GPU residency.", coords);
            }
        }
    }

    // TODO: Implement chunk despawning for very distant chunks
    // TODO: 为非常遥远的区块实现实体销毁
}
