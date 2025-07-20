use crate::{FogSystems, prelude::*};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 序列化格式
/// Serialization format  
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "format-messagepack",
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(
    feature = "format-bincode",
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum SerializationFormat {
    /// JSON格式 - 人类可读但体积较大
    /// JSON format - human readable but larger
    Json,
    /// MessagePack格式 - 二进制高效格式
    /// MessagePack format - binary efficient format
    #[cfg(feature = "format-messagepack")]
    MessagePack,
    /// Bincode格式 - Rust原生二进制格式
    /// Bincode format - Rust native binary format  
    #[cfg(feature = "format-bincode")]
    Bincode,
}

#[allow(clippy::derivable_impls)]
impl Default for SerializationFormat {
    fn default() -> Self {
        // 优先使用高效的二进制格式
        // Prefer efficient binary formats
        #[cfg(feature = "format-bincode")]
        return SerializationFormat::Bincode;

        #[cfg(all(not(feature = "format-bincode"), feature = "format-messagepack"))]
        return SerializationFormat::MessagePack;

        SerializationFormat::Json
    }
}

/// 雾效持久化保存数据
/// Fog of war persistence save data
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FogOfWarSaveData {
    /// 保存时间戳
    /// Save timestamp
    pub timestamp: u64,
    /// 已探索的区块数据
    /// Explored chunk data
    pub chunks: Vec<ChunkSaveData>,
    /// 元数据（可选）
    /// Metadata (optional)
    pub metadata: Option<SaveMetadata>,
}

/// 单个区块的保存数据
/// Save data for a single chunk
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChunkSaveData {
    /// 区块坐标
    /// Chunk coordinates
    pub coords: IVec2,
    /// 可见性状态
    /// Visibility state
    pub visibility: ChunkVisibility,
    /// 原始纹理层索引（用于恢复时保持正确的位置映射）
    /// Original texture layer indices (for maintaining correct position mapping during restoration)
    pub fog_layer_index: Option<u32>,
    pub snapshot_layer_index: Option<u32>,
    /// 雾效纹理数据（可选，用于部分可见的区块）
    /// Fog texture data (optional, for partially visible chunks)
    pub fog_data: Option<Vec<u8>>,
    /// 快照纹理数据（可选）
    /// Snapshot texture data (optional)
    pub snapshot_data: Option<Vec<u8>>,
}

/// 保存元数据
/// Save metadata
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SaveMetadata {
    /// 插件版本
    /// Plugin version
    pub plugin_version: String,
    /// 区块大小（用于验证）
    /// Chunk size (for validation)
    pub chunk_size: UVec2,
    /// 每个区块的纹理分辨率（用于验证）
    /// Texture resolution per chunk (for validation)
    pub texture_resolution: UVec2,
    /// 地图名称或 ID（可选）
    /// Map name or ID (optional)
    pub map_id: Option<String>,
}

/// 请求保存雾效数据的事件
/// Event to request saving fog of war data
#[derive(Event, Debug, Clone)]
pub struct SaveFogOfWarRequest {
    /// 是否包含纹理数据
    /// Whether to include texture data
    pub include_texture_data: bool,
    /// 序列化格式（None使用默认格式）
    /// Serialization format (None uses default)
    pub format: Option<SerializationFormat>,
}

/// 请求加载雾效数据的事件
/// Event to request loading fog of war data
#[derive(Event, Debug, Clone)]
pub struct LoadFogOfWarRequest {
    /// 要加载的序列化数据
    /// Serialized data to load
    pub data: Vec<u8>,
    /// 数据格式（None会尝试自动检测）
    /// Data format (None will try auto-detection)
    pub format: Option<SerializationFormat>,
}

/// 雾效数据保存完成事件
/// Event emitted when fog of war data is saved
#[derive(Event, Debug, Clone)]
pub struct FogOfWarSaved {
    /// 序列化的数据
    /// Serialized data
    pub data: Vec<u8>,
    /// 使用的序列化格式
    /// Serialization format used
    pub format: SerializationFormat,
    /// 保存的区块数量
    /// Number of chunks saved
    pub chunk_count: usize,
}

/// 雾效数据加载完成事件
/// Event emitted when fog of war data is loaded
#[derive(Event, Debug, Clone)]
pub struct FogOfWarLoaded {
    /// 加载的区块数量
    /// Number of chunks loaded
    pub chunk_count: usize,
    /// 加载过程中的任何警告
    /// Any warnings during loading
    pub warnings: Vec<String>,
}

/// 正在进行的保存操作状态
/// Ongoing save operation state
#[derive(Resource, Debug, Default)]
pub struct PendingSaveOperations {
    /// 当前等待GPU数据的保存操作（单一操作）
    /// Current save operation waiting for GPU data (single operation)
    pub pending_save: Option<PendingSaveData>,
}

/// 单个保存操作的状态
/// State of a single save operation
#[derive(Debug)]
pub struct PendingSaveData {
    /// 是否包含纹理数据
    /// Whether to include texture data
    pub include_texture_data: bool,
    /// 序列化格式
    /// Serialization format
    pub format: SerializationFormat,
    /// 需要等待的区块坐标
    /// Chunk coordinates to wait for
    pub awaiting_chunks: std::collections::HashSet<IVec2>,
    /// 已收到的GPU数据
    /// GPU data received so far
    pub received_data: HashMap<IVec2, (Vec<u8>, Vec<u8>)>, // (fog_data, snapshot_data)
    /// 保存的区块信息（不包含纹理数据）
    /// Chunk information to save (without texture data)
    pub chunk_info: Vec<(IVec2, ChunkVisibility, Option<u32>, Option<u32>)>, // (coords, visibility, fog_idx, snap_idx)
}

/// 雾效持久化错误
/// Fog of war persistence error
#[derive(Debug, Clone)]
pub enum PersistenceError {
    /// 序列化失败
    /// Serialization failed
    SerializationFailed(String),
    /// 反序列化失败
    /// Deserialization failed
    DeserializationFailed(String),
    /// 版本不匹配
    /// Version mismatch
    VersionMismatch { expected: String, found: String },
    /// 无效的区块大小
    /// Invalid chunk size
    InvalidChunkSize { expected: UVec2, found: UVec2 },
    /// 无效的纹理分辨率
    /// Invalid texture resolution
    InvalidTextureResolution { expected: UVec2, found: UVec2 },
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceError::SerializationFailed(msg) => {
                write!(f, "Serialization failed: {msg}")
            }
            PersistenceError::DeserializationFailed(msg) => {
                write!(f, "Deserialization failed: {msg}")
            }
            PersistenceError::VersionMismatch { expected, found } => {
                write!(f, "Version mismatch: expected {expected}, found {found}")
            }
            PersistenceError::InvalidChunkSize { expected, found } => {
                write!(
                    f,
                    "Invalid chunk size: expected {expected:?}, found {found:?}"
                )
            }
            PersistenceError::InvalidTextureResolution { expected, found } => {
                write!(
                    f,
                    "Invalid texture resolution: expected {expected:?}, found {found:?}"
                )
            }
        }
    }
}

impl std::error::Error for PersistenceError {}

/// 从保存的数据恢复雾效状态
/// Restore fog of war state from saved data
pub fn load_save_data(
    data: &FogOfWarSaveData,
    settings: &FogMapSettings,
    cache: &mut ChunkStateCache,
    commands: &mut Commands,
    chunk_manager: &mut ChunkEntityManager,
    texture_manager: &mut TextureArrayManager,
    images: &mut Assets<Image>,
) -> Result<usize, PersistenceError> {
    // 验证元数据（如果存在）
    // Validate metadata (if present)
    if let Some(metadata) = &data.metadata {
        if metadata.chunk_size != settings.chunk_size {
            return Err(PersistenceError::InvalidChunkSize {
                expected: settings.chunk_size,
                found: metadata.chunk_size,
            });
        }
        if metadata.texture_resolution != settings.texture_resolution_per_chunk {
            return Err(PersistenceError::InvalidTextureResolution {
                expected: settings.texture_resolution_per_chunk,
                found: metadata.texture_resolution,
            });
        }
    }

    // 注意：加载数据时清除当前状态，但保存时不应该重置任何状态
    // Note: Clear current state when loading data, but saving should not reset any state
    cache.reset_all();

    let mut loaded_count = 0;

    // 恢复区块状态
    // Restore chunk states
    for chunk_data in &data.chunks {
        // 添加到已探索区块集合
        // Add to explored chunks set
        cache.explored_chunks.insert(chunk_data.coords);

        if chunk_data.visibility == ChunkVisibility::Visible {
            cache.visible_chunks.insert(chunk_data.coords);
        }

        // 如果需要，创建区块实体
        // Create chunk entity if needed
        let layer_indices = if let (Some(fog_idx), Some(snap_idx)) =
            (chunk_data.fog_layer_index, chunk_data.snapshot_layer_index)
        {
            // 尝试恢复到原始层索引
            // Try to restore to original layer indices
            if texture_manager.allocate_specific_layer_indices(chunk_data.coords, fog_idx, snap_idx)
            {
                Some((fog_idx, snap_idx))
            } else {
                // 如果原始索引不可用，分配新的索引
                // If original indices not available, allocate new ones
                warn!(
                    "Original layer indices F{} S{} not available for chunk {:?}, allocating new ones",
                    fog_idx, snap_idx, chunk_data.coords
                );
                texture_manager.allocate_layer_indices(chunk_data.coords)
            }
        } else {
            // 没有保存层索引，分配新的
            // No saved layer indices, allocate new ones
            texture_manager.allocate_layer_indices(chunk_data.coords)
        };

        if let Some((fog_idx, snap_idx)) = layer_indices {
            let world_min = chunk_data.coords.as_vec2() * settings.chunk_size.as_vec2();
            let world_bounds =
                Rect::from_corners(world_min, world_min + settings.chunk_size.as_vec2());

            let chunk_image = FogChunkImage::from_setting_raw(images, settings);

            // 恢复纹理数据（如果有）
            // Restore texture data (if available)
            if let Some(fog_data) = &chunk_data.fog_data
                && let Some(fog_image) = images.get_mut(&chunk_image.fog_image_handle)
            {
                fog_image.data = Some(fog_data.clone());
            }

            if let Some(snapshot_data) = &chunk_data.snapshot_data
                && let Some(snapshot_image) = images.get_mut(&chunk_image.snapshot_image_handle)
            {
                snapshot_image.data = Some(snapshot_data.clone());
            }

            let entity = commands
                .spawn((
                    FogChunk {
                        coords: chunk_data.coords,
                        fog_layer_index: Some(fog_idx),
                        snapshot_layer_index: Some(snap_idx),
                        state: ChunkState {
                            visibility: chunk_data.visibility,
                            memory_location: ChunkMemoryLocation::Cpu, // 将在后续帧中上传到 GPU
                        },
                        world_bounds,
                    },
                    chunk_image,
                ))
                .id();

            chunk_manager.map.insert(chunk_data.coords, entity);
            loaded_count += 1;
        }
    }

    Ok(loaded_count)
}

/// 系统：处理保存雾效数据的请求
/// System: Handle fog of war save requests
pub fn save_fog_of_war_system(
    mut save_events: EventReader<SaveFogOfWarRequest>,
    mut pending_saves: ResMut<PendingSaveOperations>,
    mut gpu_to_cpu_requests: ResMut<GpuToCpuCopyRequests>,
    mut saved_events: EventWriter<FogOfWarSaved>,
    settings: Res<FogMapSettings>,
    cache: Res<ChunkStateCache>,
    chunks: Query<&FogChunk>,
    texture_manager: Res<TextureArrayManager>,
) {
    for event in save_events.read() {
        info!(
            "Starting save (include_texture_data: {})",
            event.include_texture_data
        );

        // 收集需要保存的区块信息
        // Collect chunk information to save
        let mut chunk_info = Vec::new();
        let mut awaiting_chunks = std::collections::HashSet::new();

        for &coords in &cache.explored_chunks {
            let visibility = if cache.visible_chunks.contains(&coords) {
                ChunkVisibility::Visible
            } else {
                ChunkVisibility::Explored
            };

            // 获取层索引
            // Get layer indices
            let (fog_idx, snap_idx) = if let Some(chunk) =
                chunks.iter().find(|c| c.coords == coords)
            {
                (chunk.fog_layer_index, chunk.snapshot_layer_index)
            } else {
                // 如果找不到区块实体，尝试从纹理管理器获取
                // If chunk entity not found, try to get from texture manager
                if let Some((fog_idx, snap_idx)) = texture_manager.get_allocated_indices(coords) {
                    (Some(fog_idx), Some(snap_idx))
                } else {
                    (None, None)
                }
            };

            chunk_info.push((coords, visibility, fog_idx, snap_idx));

            // 如果需要纹理数据且区块在GPU上，请求GPU到CPU传输
            // If texture data needed and chunk is on GPU, request GPU-to-CPU transfer
            if event.include_texture_data
                && visibility != ChunkVisibility::Unexplored
                && let (Some(fog_layer_idx), Some(snap_layer_idx)) = (fog_idx, snap_idx)
            {
                // 请求GPU到CPU传输
                // Request GPU-to-CPU transfer
                gpu_to_cpu_requests.requests.push(GpuToCpuCopyRequest {
                    chunk_coords: coords,
                    fog_layer_index: fog_layer_idx,
                    snapshot_layer_index: snap_layer_idx,
                });

                awaiting_chunks.insert(coords);
                info!(
                    "Requesting GPU-to-CPU transfer for chunk {:?} (F{}, S{})",
                    coords, fog_layer_idx, snap_layer_idx
                );
            }
        }

        // 如果不需要等待GPU数据，立即保存
        // If no GPU data needed, save immediately
        if awaiting_chunks.is_empty() {
            match create_save_data_immediate(
                &settings,
                chunk_info,
                HashMap::new(),
                event.include_texture_data,
            ) {
                Ok(save_data) => {
                    let format = event.format.unwrap_or_default();
                    complete_save_operation(save_data, format, &mut saved_events);
                }
                Err(e) => {
                    error!("Failed to create save data: {}", e);
                }
            }
        } else {
            // 创建挂起的保存操作
            // Create pending save operation
            let pending = PendingSaveData {
                include_texture_data: event.include_texture_data,
                format: event.format.unwrap_or_default(),
                awaiting_chunks: awaiting_chunks.clone(),
                received_data: HashMap::new(),
                chunk_info,
            };

            pending_saves.pending_save = Some(pending);
            info!(
                "Created pending save, waiting for {} chunks",
                awaiting_chunks.len()
            );
        }
    }
}

/// 系统：处理GPU数据就绪事件，完成挂起的保存操作
/// System: Handle GPU data ready events and complete pending save operations
pub fn handle_gpu_data_ready_system(
    mut gpu_ready_events: EventReader<ChunkGpuDataReady>,
    mut pending_saves: ResMut<PendingSaveOperations>,
    mut saved_events: EventWriter<FogOfWarSaved>,
    settings: Res<FogMapSettings>,
) {
    for event in gpu_ready_events.read() {
        // 检查是否有挂起的保存操作等待此数据
        // Check if there's a pending save operation waiting for this data
        if let Some(pending) = &mut pending_saves.pending_save
            && pending.awaiting_chunks.contains(&event.chunk_coords)
        {
            // 存储接收到的数据
            // Store received data
            pending.received_data.insert(
                event.chunk_coords,
                (event.fog_data.clone(), event.snapshot_data.clone()),
            );

            // 从等待列表中移除
            // Remove from awaiting list
            pending.awaiting_chunks.remove(&event.chunk_coords);

            info!(
                "Received GPU data for chunk {:?}. Still waiting for {} chunks",
                event.chunk_coords,
                pending.awaiting_chunks.len()
            );

            // 检查是否所有数据都已就绪
            // Check if all data is ready
            if pending.awaiting_chunks.is_empty() {
                // 完成保存操作
                // Complete save operation
                if let Some(pending) = pending_saves.pending_save.take() {
                    match create_save_data_immediate(
                        &settings,
                        pending.chunk_info,
                        pending.received_data,
                        pending.include_texture_data,
                    ) {
                        Ok(save_data) => {
                            complete_save_operation(save_data, pending.format, &mut saved_events);
                        }
                        Err(e) => {
                            error!("Failed to complete save: {}", e);
                        }
                    }
                }
            }
        }
    }
}

/// 立即创建保存数据（使用已有的纹理数据）
/// Create save data immediately (using available texture data)
fn create_save_data_immediate(
    settings: &FogMapSettings,
    chunk_info: Vec<(IVec2, ChunkVisibility, Option<u32>, Option<u32>)>, // (coords, visibility, fog_idx, snap_idx)
    texture_data: HashMap<IVec2, (Vec<u8>, Vec<u8>)>, // (fog_data, snapshot_data)
    include_texture_data: bool,
) -> Result<FogOfWarSaveData, PersistenceError> {
    let mut chunk_data = Vec::new();

    for (coords, visibility, fog_idx, snap_idx) in chunk_info {
        let (fog_data, snapshot_data) = if include_texture_data {
            // 使用从GPU传输的真实数据
            // Use real data from GPU transfer
            if let Some((fog_bytes, snap_bytes)) = texture_data.get(&coords) {
                let fog_data = if visibility != ChunkVisibility::Unexplored {
                    Some(fog_bytes.clone())
                } else {
                    None
                };

                let snapshot_data = if visibility == ChunkVisibility::Explored {
                    Some(snap_bytes.clone())
                } else {
                    None
                };

                (fog_data, snapshot_data)
            } else {
                // 如果没有GPU数据，则不包含纹理数据
                // If no GPU data available, don't include texture data
                (None, None)
            }
        } else {
            (None, None)
        };

        chunk_data.push(ChunkSaveData {
            coords,
            visibility,
            fog_layer_index: fog_idx,
            snapshot_layer_index: snap_idx,
            fog_data,
            snapshot_data,
        });
    }

    Ok(FogOfWarSaveData {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        chunks: chunk_data,
        metadata: Some(SaveMetadata {
            plugin_version: env!("CARGO_PKG_VERSION").to_string(),
            chunk_size: settings.chunk_size,
            texture_resolution: settings.texture_resolution_per_chunk,
            map_id: None,
        }),
    })
}

/// 完成保存操作，序列化数据并发送事件
/// Complete save operation, serialize data and send event
fn complete_save_operation(
    save_data: FogOfWarSaveData,
    format: SerializationFormat,
    saved_events: &mut EventWriter<FogOfWarSaved>,
) {
    let result = match format {
        SerializationFormat::Json => serde_json::to_vec(&save_data)
            .map_err(|e| PersistenceError::SerializationFailed(e.to_string())),
        #[cfg(feature = "format-messagepack")]
        SerializationFormat::MessagePack => rmp_serde::to_vec(&save_data)
            .map_err(|e| PersistenceError::SerializationFailed(e.to_string())),
        #[cfg(feature = "format-bincode")]
        SerializationFormat::Bincode => bincode::serialize(&save_data)
            .map_err(|e| PersistenceError::SerializationFailed(e.to_string())),
    };

    match result {
        Ok(data) => {
            let chunk_count = save_data.chunks.len();
            info!(
                "Save completed successfully using {:?} format: {} chunks, {} bytes",
                format,
                chunk_count,
                data.len()
            );

            saved_events.write(FogOfWarSaved {
                data,
                format,
                chunk_count,
            });
        }
        Err(e) => {
            error!(
                "Failed to serialize save data using {:?} format: {}",
                format, e
            );
        }
    }
}

/// 系统：处理加载雾效数据的请求
/// System: Handle fog of war load requests
pub fn load_fog_of_war_system(
    mut load_events: EventReader<LoadFogOfWarRequest>,
    mut loaded_events: EventWriter<FogOfWarLoaded>,
    mut commands: Commands,
    settings: Res<FogMapSettings>,
    mut cache: ResMut<ChunkStateCache>,
    mut chunk_manager: ResMut<ChunkEntityManager>,
    mut texture_manager: ResMut<TextureArrayManager>,
    mut images: ResMut<Assets<Image>>,
    existing_chunks: Query<Entity, With<FogChunk>>,
) {
    for event in load_events.read() {
        let mut warnings = Vec::new();

        // 根据格式反序列化数据
        // Deserialize data based on format
        let format = event.format.unwrap_or_else(|| {
            // 尝试自动检测格式
            // Try to auto-detect format
            if event.data.starts_with(b"{") || event.data.starts_with(b"[") {
                SerializationFormat::Json
            } else {
                // 默认假设为bincode
                // Default assume bincode
                #[cfg(feature = "format-bincode")]
                return SerializationFormat::Bincode;
                #[cfg(not(feature = "format-bincode"))]
                return SerializationFormat::Json;
            }
        });

        let result = match format {
            SerializationFormat::Json => serde_json::from_slice::<FogOfWarSaveData>(&event.data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string())),
            #[cfg(feature = "format-messagepack")]
            SerializationFormat::MessagePack => {
                rmp_serde::from_slice::<FogOfWarSaveData>(&event.data)
                    .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
            }
            #[cfg(feature = "format-bincode")]
            SerializationFormat::Bincode => bincode::deserialize::<FogOfWarSaveData>(&event.data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string())),
        };

        match result {
            Ok(save_data) => {
                // 清除现有的区块实体
                // Clear existing chunk entities
                for entity in existing_chunks.iter() {
                    commands.entity(entity).despawn();
                }
                chunk_manager.map.clear();
                texture_manager.clear_all_layers();

                // 加载保存的数据
                // Load saved data
                match load_save_data(
                    &save_data,
                    &settings,
                    &mut cache,
                    &mut commands,
                    &mut chunk_manager,
                    &mut texture_manager,
                    &mut images,
                ) {
                    Ok(loaded_count) => {
                        info!("Loaded fog of war data: {} chunks", loaded_count);

                        // 检查是否有区块未能加载
                        // Check if any chunks failed to load
                        if loaded_count < save_data.chunks.len() {
                            warnings.push(format!(
                                "Only loaded {} out of {} chunks (texture array may be full)",
                                loaded_count,
                                save_data.chunks.len()
                            ));
                        }

                        loaded_events.write(FogOfWarLoaded {
                            chunk_count: loaded_count,
                            warnings,
                        });
                    }
                    Err(e) => {
                        error!("Failed to load fog of war data: {}", e);
                    }
                }
            }
            Err(e) => {
                error!(
                    "Failed to deserialize fog of war data using {:?} format: {}",
                    format, e
                );
            }
        }
    }
}

/// 插件扩展，用于添加持久化功能
/// Plugin extension for adding persistence functionality
pub struct FogOfWarPersistencePlugin;

impl Plugin for FogOfWarPersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SaveFogOfWarRequest>()
            .add_event::<LoadFogOfWarRequest>()
            .add_event::<FogOfWarSaved>()
            .add_event::<FogOfWarLoaded>()
            .init_resource::<PendingSaveOperations>()
            .add_systems(
                Update,
                (
                    save_fog_of_war_system,
                    handle_gpu_data_ready_system,
                    load_fog_of_war_system,
                )
                    .in_set(FogSystems::Persistence),
            );
    }
}
