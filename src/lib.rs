use self::prelude::*;
use bevy::asset::RenderAssetUsages;
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
                // update_chunk_component_state, // Sync cache state to components / 将缓存状态同步到组件
            )
                .in_set(FogSystemSet::UpdateChunkState),
        );

        app.add_plugins(chunk::ChunkManagerPlugin)
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
