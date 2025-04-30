use crate::{sync::SyncChunk, prelude::SyncChunkComplete};
use bevy_app::prelude::*;
use bevy_asset::{Assets, Handle, RenderAssetUsages};
use bevy_ecs::prelude::*;
use bevy_image::Image;
use bevy_log::{info, warn};
use bevy_math::prelude::*;
use bevy_platform::collections::{HashMap, HashSet};
use bevy_reflect::Reflect;
use bevy_render::{
    extract_resource::ExtractResourcePlugin,
    prelude::*,
    render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    renderer::RenderDevice,
};
use bevy_render_macros::{ExtractComponent, ExtractResource};
use bevy_transform::prelude::GlobalTransform;
use bevy_utils::prelude::*;
use std::{collections::VecDeque, time::Instant};
use bevy_render::extract_component::ExtractComponentPlugin;

/// 区块坐标类型，用于标识区块的二维坐标
/// Chunk coordinate type, used to identify the 2D coordinates of a chunk
pub type ChunkCoord = IVec2;

/// 默认区块大小，单位为网格数量
/// Default chunk size in grid units
pub const DEFAULT_CHUNK_SIZE: u32 = 256;

/// 区块管理系统插件
/// Chunk management system plugin
pub struct ChunkManagerPlugin;

impl Plugin for ChunkManagerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkManager>()
            .add_plugins(ExtractComponentPlugin::<FogOfWarCamera>::default())
            .add_plugins(ExtractResourcePlugin::<ChunkManager>::default())
            .register_type::<MapChunk>()
            .register_type::<InCameraView>()
            .add_systems(PreUpdate, (manage_chunks_by_viewport).chain())
            .add_systems(PreUpdate, update_chunk_visibility);
    }
}

/// 地图区块组件，代表一个空间区域的迷雾和可见性数据
/// Map chunk component, represents fog and visibility data for a spatial region
#[derive(Component, ExtractComponent, Reflect, Debug, Clone)]
pub struct MapChunk {
    /// 区块坐标
    /// Chunk coordinates
    pub chunk_coord: ChunkCoord,

    /// 区块尺寸
    /// Chunk size
    pub size: UVec2,

    pub layer_index: Option<u32>,
    pub screen_index: Option<u32>,

    /// 是否加载
    /// Whether the chunk is loaded
    pub loaded: bool,

    /// 区块的世界空间边界（以像素/单位为单位）
    /// World space boundaries of the chunk (in pixels/units)
    pub world_bounds: Rect,

    pub texture: Handle<Image>,

    pub active_time: Instant,
}

impl MapChunk {
    pub fn unique_id(&self) -> u32 {
        let ox = (self.chunk_coord.x + 32768) as u32;
        let oy = (self.chunk_coord.y + 32768) as u32;
        (ox << 16) | (oy & 0xFFFF)
    }
    /// 创建一个新的地图区块
    /// Create a new map chunk
    pub fn new(
        chunk_coord: ChunkCoord,
        size: UVec2,
        tile_size: f32,
        texture: Handle<Image>,
    ) -> Self {
        let min = Vec2::new(
            chunk_coord.x as f32 * size.x as f32 * tile_size,
            chunk_coord.y as f32 * size.y as f32 * tile_size,
        );
        let max = min + Vec2::new(size.x as f32 * tile_size, size.y as f32 * tile_size);

        Self {
            chunk_coord,
            size,
            layer_index: None,
            screen_index: None,
            loaded: true,
            world_bounds: Rect { min, max },
            texture,
            active_time: Instant::now(),
        }
    }

    /// 将世界坐标转换为区块内的局部坐标
    /// Convert world coordinates to local coordinates within the chunk
    pub fn world_to_local(&self, world_pos: Vec2, tile_size: f32) -> Option<UVec2> {
        if !self.world_bounds.contains(world_pos) {
            return None;
        }

        let relative_pos = world_pos - self.world_bounds.min;
        let local_x = (relative_pos.x / tile_size) as u32;
        let local_y = (relative_pos.y / tile_size) as u32;

        if local_x < self.size.x && local_y < self.size.y {
            Some(UVec2::new(local_x, local_y))
        } else {
            None
        }
    }

    /// 判断一个世界坐标是否在该区块内
    /// Check if a world coordinate is within this chunk
    pub fn contains_world_pos(&self, world_pos: Vec2) -> bool {
        self.world_bounds.contains(world_pos)
    }
}

/// 可见性状态枚举，表示区块中每个格子的可见性
/// Visibility state enum, represents the visibility of each cell in a chunk
#[derive(Debug, Reflect, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityState {
    /// 未探索 - 完全不可见
    /// Unexplored - completely invisible
    Unexplored,

    /// 已探索 - 曾经可见，现在仅显示静态内容
    /// Explored - previously visible, now only shows static content
    Explored,

    /// 可见 - 当前完全可见，显示所有动态内容
    /// Visible - currently fully visible, shows all dynamic content
    Visible,
}

impl Default for VisibilityState {
    fn default() -> Self {
        Self::Unexplored
    }
}

/// 区块管理器，管理所有加载的区块
/// Chunk manager, manages all loaded chunks
#[derive(Resource, ExtractResource, Clone, Debug)]
pub struct ChunkManager {
    /// 所有已加载区块的映射，从区块坐标到实体ID
    /// Map of all loaded chunks, from chunk coordinates to entity ID
    pub loaded_chunks: HashMap<ChunkCoord, Entity>,
    pub chunk_in_views: usize,

    pub chunks_per_row: usize,
    pub chunks_per_cols: usize,

    /// 区块大小
    /// Chunk size
    pub chunk_size: UVec2,

    /// 地块大小
    /// Tile size
    pub tile_size: f32,

    /// 当前帧时间戳
    /// Current frame timestamp
    pub current_timestamp: u64,

    /// FIFO queue
    pub layer_queue: VecDeque<u32>,
    pub max_layer_count: u32,
}

impl Default for ChunkManager {
    fn default() -> Self {
        let max_layer_count = 128;
        let mut layer_queue = VecDeque::with_capacity(max_layer_count as usize);
        for i in 0..max_layer_count {
            layer_queue.push_back(i);
        }
        Self {
            chunk_size: UVec2::splat(DEFAULT_CHUNK_SIZE),
            tile_size: 1.0, // Default tile size, adjust as needed in your app setup
            loaded_chunks: HashMap::new(),
            current_timestamp: 0,
            layer_queue,
            chunk_in_views: 0,
            chunks_per_row: 0,
            chunks_per_cols: 0,
            max_layer_count,
        }
    }
}

impl ChunkManager {
    pub fn update_layer(&mut self, chunk: &mut MapChunk, new_screen_index: u32) {
        chunk.active_time = Instant::now();

        if chunk.screen_index.is_none() {
            // 是从屏幕外进入屏幕
            if let Some(layer_id) = self.layer_queue.pop_front() {
                // println!("screen_index: {new_screen_index} new layer_id: {}", layer_id);
                chunk.layer_index = Some(layer_id);
            } else {
                warn!("not enough layers to update");
            }
        }

        chunk.screen_index.replace(new_screen_index);
    }

    pub fn unload_layer(&mut self, chunk: &mut MapChunk) {
        if let Some(layer_id) = chunk.layer_index {
            self.layer_queue.push_back(layer_id);
            chunk.screen_index = None;
            chunk.layer_index = None;
        }
    }

    /// 将世界坐标转换为区块坐标
    /// Convert world coordinates to chunk coordinates
    pub fn world_to_chunk_coord(&self, world_pos: Vec2) -> ChunkCoord {
        let chunk_world_size = Vec2::new(
            self.chunk_size.x as f32 * self.tile_size,
            self.chunk_size.y as f32 * self.tile_size,
        );

        ChunkCoord::new(
            (world_pos.x / chunk_world_size.x).floor() as i32,
            (world_pos.y / chunk_world_size.y).floor() as i32,
        )
    }

    /// 检查区块是否已加载
    /// Check if a chunk is loaded
    pub fn is_chunk_loaded(&self, chunk_coord: &ChunkCoord) -> bool {
        self.loaded_chunks.contains_key(chunk_coord)
    }

    /// 获取已加载区块的实体ID
    /// Get the entity ID of a loaded chunk
    pub fn get_chunk_entity(&self, chunk_coord: ChunkCoord) -> Option<Entity> {
        self.loaded_chunks.get(&chunk_coord).copied()
    }

    /// 添加已加载区块
    /// Add a loaded chunk
    pub fn add_chunk(&mut self, chunk_coord: ChunkCoord, entity: Entity) {
        self.loaded_chunks.insert(chunk_coord, entity);
    }

    /// 移除已加载区块
    /// Remove a loaded chunk
    pub fn remove_chunk(&mut self, chunk_coord: ChunkCoord) {
        self.loaded_chunks.remove(&chunk_coord);
    }

    /// 增加时间戳
    /// Increment the timestamp
    pub fn increment_timestamp(&mut self) {
        self.current_timestamp = self.current_timestamp.wrapping_add(1);
    }

    /// 将区块坐标转换为世界坐标（区块的左下角）
    /// Convert chunk coordinates to world coordinates (bottom-left corner of the chunk)
    pub fn chunk_coord_to_world(&self, chunk_coord: ChunkCoord) -> Vec2 {
        Vec2::new(
            chunk_coord.x as f32 * self.chunk_size.x as f32 * self.tile_size,
            chunk_coord.y as f32 * self.chunk_size.y as f32 * self.tile_size,
        )
    }
}

/// 基于屏幕视口的区块管理系统，负责动态加载区块
/// Viewport-based chunk management system, responsible for dynamically loading chunks
fn manage_chunks_by_viewport(
    mut commands: Commands,
    mut chunk_manager: ResMut<ChunkManager>,
    camera_query: Query<(&Camera, &GlobalTransform, &Projection)>,
    mut images: ResMut<Assets<Image>>,
    render_device: Res<RenderDevice>,
) {
    // 获取相机和投影信息
    // Get camera and projection information
    let Some((camera, camera_transform, projection)) = camera_query.iter().next() else {
        return; // 没有相机时不进行处理 / Do nothing when there is no camera
    };

    // 获取相机位置
    // Get camera position
    let camera_pos = camera_transform.translation().truncate();

    let Projection::Orthographic(projection) = &projection else {
        return;
    };

    // 计算视口的世界空间边界
    // Calculate viewport boundaries in world space
    let viewport_size = camera
        .logical_viewport_size()
        .unwrap_or(Vec2::new(1280.0, 720.0));
    let half_size = viewport_size * 0.5 * projection.scale;

    // 计算视口的世界空间边界
    // Calculate viewport boundaries in world space
    let viewport_rect = Rect {
        min: camera_pos - half_size,
        max: camera_pos + half_size,
    };

    // 计算视口覆盖的区块坐标范围
    // Calculate chunk coordinates range covered by the viewport
    let min_chunk = chunk_manager.world_to_chunk_coord(viewport_rect.min);
    let max_chunk = chunk_manager.world_to_chunk_coord(viewport_rect.max);

    // 添加一个额外的缓冲区，确保边缘区块也被加载
    // Add an extra buffer to ensure edge chunks are also loaded
    let buffer = 1;
    let load_min = ChunkCoord::new(min_chunk.x - buffer, min_chunk.y - buffer);
    let load_max = ChunkCoord::new(max_chunk.x + buffer, max_chunk.y + buffer);

    // 计算需要加载的所有区块
    // Calculate all chunks that need to be loaded
    let mut chunks_to_load = HashSet::new();
    for x in load_min.x..=load_max.x {
        for y in load_min.y..=load_max.y {
            chunks_to_load.insert(ChunkCoord::new(x, y));
        }
    }

    // 加载新区块，但不重复创建已经存在的区块
    // Load new chunks, but don't recreate existing ones
    for chunk_coord in chunks_to_load {
        if !chunk_manager.is_chunk_loaded(&chunk_coord) {
            let chunk_size = chunk_manager.chunk_size;
            let tile_size = chunk_manager.tile_size;

            // Create a storage texture with some data
            let size = Extent3d {
                width: chunk_size.x,
                height: chunk_size.y,
                ..default()
            };
            let mut image = Image::new_fill(
                size,
                TextureDimension::D2,
                &[0],
                TextureFormat::R8Unorm,
                RenderAssetUsages::RENDER_WORLD,
            );

            image.texture_descriptor.usage |=
                TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING;
            let image1 = images.add(image.clone());

            // 创建区块实体
            // Create chunk entity
            let entity = commands
                .spawn((
                    MapChunk::new(chunk_coord, chunk_size, tile_size, image1.clone()),
                    SyncChunk::new(chunk_coord, image1.clone(), size, &render_device),
                    // ImageCopier::new(chunk_coord, image1.clone(), size, &render_device),
                    InCameraView::default(),
                    // SyncChunk {
                    //     chunk_coord,
                    //     texture: image1.clone(),
                    // },
                    Name::new(format!("Chunk ({}, {})", chunk_coord.x, chunk_coord.y)), // Also re-add Name for debugging
                ))
                .observe(on_texture_download)
                .id();

            // 更新管理器和索引
            // Update manager and index
            chunk_manager.add_chunk(chunk_coord, entity);
        }
    }
}

fn on_texture_download(
    trigger: Trigger<SyncChunkComplete>,
    mut images: ResMut<Assets<Image>>,
    mut q_chunk: Query<(&MapChunk, &mut SyncChunk)>,
) {
    let (chunk, mut chunk_texture) = q_chunk.get_mut(trigger.target()).unwrap();

    let data: Vec<u8> = trigger.event().data.clone();
    chunk_texture.need_upload = false;
    chunk_texture.need_download = false;
    if chunk.chunk_coord == ChunkCoord::new(-1, -1) {
        if data.iter().all(|&x| x == 0) {
            info!("All pixels are zero");
        } else {
            info!("Some pixels are not zero");
        }
    }
    if let Some(image) = images.get_mut(&chunk_texture.src) {
        image.data = Some(data);
        chunk_texture.buffer = trigger.event().buffer.clone();
    }
}

/// 更新区块可见性系统
/// Update chunk visibility system
fn update_chunk_visibility(
    mut chunk_manager: ResMut<ChunkManager>,
    cameras: Query<(&Camera, &GlobalTransform, &Projection), Changed<GlobalTransform>>,
    mut chunks: Query<(Entity, &mut MapChunk, &mut SyncChunk, Option<&InCameraView>)>,
    mut commands: Commands,
) {
    // 遍历所有相机，更新区块的可见性状态
    // Iterate through all cameras and update chunk visibility status
    for (camera, camera_transform, projection) in cameras.iter() {
        if !camera.is_active {
            continue;
        }

        if let Projection::Orthographic(projection) = projection {
            // 计算相机的视口矩形（世界坐标）
            // Calculate camera viewport rectangle (world coordinates)
            let camera_pos = camera_transform.translation().truncate();
            let half_width = projection.area.width() * 0.5;
            let half_height = projection.area.height() * 0.5;

            let viewport_rect = Rect {
                min: camera_pos - Vec2::new(half_width, half_height),
                max: camera_pos + Vec2::new(half_width, half_height),
            };

            // 添加一点额外边距，确保边缘区块也被标记为可见
            // Add a little extra margin to ensure edge chunks are also marked as visible
            let margin = chunk_manager.chunk_size.x as f32 * 0.2; // Increase margin significantly to ensure chunks near the viewport edge are included
            let expanded_viewport = Rect {
                min: viewport_rect.min - Vec2::splat(margin * chunk_manager.tile_size),
                max: viewport_rect.max + Vec2::splat(margin * chunk_manager.tile_size),
            };
            let ordered_coords = ordered_chunks_in_view(
                expanded_viewport,
                chunk_manager.chunk_size,
                chunk_manager.tile_size,
            );

            let mut count = 0;

            for (entity, mut chunk, mut sync_texture, opt_in_view) in chunks.iter_mut() {
                let mut in_view = false;
                'f: for (screen_index, order_chunk) in ordered_coords.iter().enumerate() {
                    if *order_chunk == chunk.chunk_coord {
                        chunk_manager.update_layer(&mut chunk, screen_index as u32);
                        sync_texture.layer_index = chunk.layer_index.unwrap();
                        in_view = true;
                        break 'f;
                    }
                }

                if in_view {
                    count += 1;
                    if sync_texture.need_upload {
                        sync_texture.need_upload = false;
                    }

                    if opt_in_view.is_none() {
                        if chunk.layer_index.is_some() {
                            sync_texture.need_upload = true;
                        }
                        commands.entity(entity).insert(InCameraView::default());

                        // chunk_manager
                        //     .sync_to_render
                        //     .push((chunk.chunk_coord, chunk.layer_index.unwrap()));
                    }
                } else {
                    if opt_in_view.is_some() {
                        if let Some(layer_index) = chunk.layer_index {
                            sync_texture.need_download = true;
                            sync_texture.layer_index = layer_index;
                        }
                        commands.entity(entity).remove::<InCameraView>();

                        chunk_manager.unload_layer(&mut chunk);
                    }
                }
            }
            chunk_manager.chunk_in_views = count;
        }
    }
}

/// 标记区块是否在相机视野中的组件
/// Component that marks whether a chunk is in camera view
#[derive(Component, ExtractComponent, Reflect, Debug, Default, Clone)]
pub struct InCameraView {
    /// 是否在相机视野中
    /// Whether the chunk is in camera view
    pub in_view: bool,

    /// 上次更新时间戳
    /// Last update timestamp
    pub last_update: u64,
}

pub fn ordered_chunks_in_view(
    expanded_viewport: Rect,
    chunk_size: UVec2,
    tile_size: f32,
) -> Vec<ChunkCoord> {
    let chunk_world_size = Vec2::new(
        chunk_size.x as f32 * tile_size,
        chunk_size.y as f32 * tile_size,
    );

    let min_chunk = IVec2::new(
        (expanded_viewport.min.x / chunk_world_size.x).floor() as i32,
        (expanded_viewport.min.y / chunk_world_size.y).floor() as i32,
    );
    let max_chunk = IVec2::new(
        (expanded_viewport.max.x / chunk_world_size.x).floor() as i32,
        (expanded_viewport.max.y / chunk_world_size.y).floor() as i32,
    );

    let mut result = Vec::new();
    // 固定顺序遍历（比如从上到下，从左到右）
    for y in min_chunk.y..=max_chunk.y {
        for x in min_chunk.x..=max_chunk.x {
            result.push(IVec2::new(x, y));
        }
    }
    result
}

#[derive(Component, ExtractComponent, Clone)]
pub struct FogOfWarCamera;
