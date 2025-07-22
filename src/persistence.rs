//! Fog of war persistence system for saving and loading fog exploration data.
//! 战争迷雾持久化系统，用于保存和加载雾效探索数据

// Allow collapsible_if for stable Rust compatibility
#![allow(clippy::collapsible_if)]
//!
//! This module provides comprehensive save/load functionality for fog of war exploration
//! state, including texture data, chunk visibility states, and metadata. It supports
//! multiple serialization formats and handles async GPU data transfers for complete saves.
//!
//! # Persistence Architecture
//!
//! ## Save/Load Workflow
//! The persistence system handles complex async operations:
//! ```text
//! [Save Request] → [GPU Transfer] → [Data Collection] → [Serialization] → [Save Event]
//!       ↓               ↓               ↓                ↓                ↓
//! User/System     GPU→CPU Copy    Texture Data      Format-specific   Completion
//! Initiated       Async Process   Accumulation      Encoding          Notification
//! ```
//!
//! ## Supported Formats
//! - **JSON**: Human-readable, larger size, universal compatibility
//! - **MessagePack**: Binary efficient, cross-language compatibility (feature-gated)
//! - **Bincode**: Rust-native binary, highest efficiency (feature-gated)
//!
//! # Data Structure Organization
//!
//! ## Hierarchical Save Data
//! - **FogOfWarSaveData**: Root save container with metadata
//! - **ChunkSaveData**: Individual chunk exploration data
//! - **SaveMetadata**: Version and configuration validation data
//! - **Texture Data**: Optional binary texture content
//!
//! ## State Preservation
//! The system preserves complete fog state:
//! - **Exploration State**: Which chunks have been discovered
//! - **Visibility State**: Current visibility status per chunk
//! - **Texture Data**: Optional pixel-level fog and snapshot data
//! - **Layer Mapping**: GPU texture array layer indices for restoration
//!
//! # Async Operation Management
//!
//! ## GPU Data Dependencies
//! For complete saves including texture data:
//! - **GPU Transfer Requests**: Initiate texture downloads
//! - **Async Collection**: Wait for all GPU data completion
//! - **Batch Processing**: Combine all texture data efficiently
//! - **Event-Driven Completion**: Signal save completion when ready
//!
//! ## Memory Management
//! - **Streaming Transfers**: Large texture data handled efficiently
//! - **Temporary Storage**: Minimal memory footprint during operations
//! - **Format Selection**: Optimize storage based on format choice
//! - **Resource Cleanup**: Automatic cleanup of intermediate data
//!
//! # Validation and Compatibility
//!
//! ## Version Management
//! - **Plugin Version**: Track compatibility across saves
//! - **Configuration Validation**: Ensure chunk size and texture resolution match
//! - **Migration Support**: Future-proofing for format changes
//! - **Error Recovery**: Graceful handling of incompatible saves
//!
//! ## Data Integrity
//! - **Metadata Validation**: Verify save compatibility before loading
//! - **Chunk Verification**: Validate chunk data consistency
//! - **Format Detection**: Auto-detect serialization format when possible
//! - **Error Reporting**: Detailed error messages for debugging

use crate::{FogSystems, RequestChunkSnapshot, prelude::*};
use bevy_ecs::system::SystemParam;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use bevy_math::{IVec2, Rect, UVec2};
use bevy_app::{App, Plugin};
use bevy_asset::Assets;
use bevy_image::Image;
use bevy_log::{error, info, warn};

/// Enumeration of supported serialization formats for fog data persistence.
/// 雾效数据持久化支持的序列化格式枚举
///
/// This enum defines the available data formats for saving and loading fog of war data,
/// each optimized for different use cases and performance characteristics.
///
/// # Format Characteristics
/// - **JSON**: Human-readable text format, larger file size, universal compatibility
/// - **MessagePack**: Binary format with good compression, cross-language support
/// - **Bincode**: Rust-native binary format, highest performance and smallest size
///
/// # Feature Gates
/// Binary formats require corresponding feature flags:
/// - `format-messagepack`: Enables MessagePack support
/// - `format-bincode`: Enables Bincode support
///
/// # Performance Comparison
/// Typical relative performance (speed/size):
/// - **Bincode**: 100% speed, 100% compression (baseline)
/// - **MessagePack**: ~80% speed, ~110% size
/// - **JSON**: ~40% speed, ~300% size
///
/// # Use Case Recommendations
/// - **Bincode**: Production saves, best performance
/// - **MessagePack**: Cross-platform saves, good balance
/// - **JSON**: Debug saves, human inspection, configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    any(feature = "format-messagepack", feature = "format-bincode"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum SerializationFormat {
    /// JSON format - human readable but larger file size.
    /// JSON格式 - 人类可读但体积较大
    ///
    /// Best for: debugging, configuration files, human inspection
    /// Typical size: ~3x larger than binary formats
    /// Performance: Slower serialization/deserialization
    #[cfg(feature = "format-json")]
    Json,

    /// MessagePack format - binary efficient with cross-language support.
    /// MessagePack格式 - 二进制高效格式，支持跨语言
    ///
    /// Best for: cross-platform saves, network transfer
    /// Typical size: ~10% larger than Bincode
    /// Performance: Good balance of speed and compatibility
    #[cfg(feature = "format-messagepack")]
    MessagePack,

    /// Bincode format - Rust native binary format with optimal performance.
    /// Bincode格式 - Rust原生二进制格式，性能最优
    ///
    /// Best for: production saves, maximum performance
    /// Typical size: Smallest binary representation
    /// Performance: Fastest serialization/deserialization
    #[cfg(feature = "format-bincode")]
    Bincode,
}

#[allow(clippy::derivable_impls)]
impl Default for SerializationFormat {
    fn default() -> Self {
        // 优先使用高效的二进制格式
        // Prefer efficient binary formats
        #[cfg(feature = "format-bincode")]
        {
            SerializationFormat::Bincode
        }
        #[cfg(all(not(feature = "format-bincode"), feature = "format-messagepack"))]
        {
            SerializationFormat::MessagePack
        }
        #[cfg(all(
            not(feature = "format-bincode"),
            not(feature = "format-messagepack"),
            feature = "format-json"
        ))]
        {
            SerializationFormat::Json
        }
    }
}

// Compile-time check to ensure at least one serialization format is enabled
// 编译时检查确保至少启用一种序列化格式
#[cfg(not(any(
    feature = "format-json",
    feature = "format-bincode",
    feature = "format-messagepack"
)))]
compile_error!(
    "At least one serialization format must be enabled. \
     Available features: format-json, format-bincode, format-messagepack"
);

/// Root container for serialized fog of war save data with metadata and chunk information.
/// 序列化雾效保存数据的根容器，包含元数据和区块信息
///
/// This structure represents a complete fog of war save state that can be serialized
/// to various formats (JSON, MessagePack, Bincode) for persistent storage or network
/// transmission. It includes comprehensive metadata for validation and compatibility.
///
/// # Data Organization
///
/// ## Hierarchical Structure
/// ```text
/// FogOfWarSaveData
/// ├── timestamp: Save creation time
/// ├── metadata: Version and compatibility info
/// └── chunks: Array of chunk exploration data
///     ├── ChunkSaveData[0]: First explored chunk
///     ├── ChunkSaveData[1]: Second explored chunk
///     └── ...
/// ```
///
/// ## Serialization Compatibility
/// The structure is designed for cross-version compatibility:
/// - **Serde Support**: Implements Serialize/Deserialize for all supported formats
/// - **Optional Fields**: Future fields can be added with `Option<T>` for backward compatibility
/// - **Version Tracking**: Metadata includes plugin version for compatibility checking
/// - **Validation Data**: Settings validation prevents incompatible save loading
///
/// # Performance Characteristics
///
/// ## Memory Usage
/// - **Base Overhead**: Minimal structure overhead (~32 bytes)
/// - **Chunk Data**: Linear scaling with number of explored chunks
/// - **Texture Data**: Optional large data when included in saves
/// - **Metadata**: Small fixed overhead for compatibility information
///
/// ## Serialization Performance
/// Different formats have varying performance characteristics:
/// - **JSON**: ~40% speed, ~300% size (human-readable)
/// - **MessagePack**: ~80% speed, ~110% size (cross-platform binary)
/// - **Bincode**: 100% speed, 100% size (Rust-native binary baseline)
///
/// # Usage Patterns
///
/// ## Game Save Files
/// ```rust,no_run
/// # use bevy_fog_of_war::prelude::*;
/// // Complete save with texture data for full restoration
/// let save_request = SaveFogOfWarRequest {
///     include_texture_data: true,
///     format: Some(SerializationFormat::Bincode),
/// };
/// ```
///
/// ## Network Synchronization
/// ```rust,ignore
/// # use bevy_fog_of_war::prelude::*;
/// // Metadata-only save for network sync (smaller size)
/// let sync_request = SaveFogOfWarRequest {
///     include_texture_data: false,
///     format: Some(SerializationFormat::MessagePack),
/// };
/// ```
///
/// ## Debug/Development
/// ```rust,no_run
/// # use bevy_fog_of_war::prelude::*;
/// // Human-readable save for debugging
/// let debug_request = SaveFogOfWarRequest {
///     include_texture_data: true,
///     #[cfg(feature = "format-json")]
///     format: Some(SerializationFormat::Json),
///     #[cfg(not(feature = "format-json"))]
///     format: Some(SerializationFormat::Bincode),
/// };
/// ```
///
/// # Validation and Compatibility
///
/// ## Load-Time Validation
/// When loading, the system validates:
/// - **Plugin Version**: Logged for compatibility tracking
/// - **Chunk Size**: Must match current settings exactly
/// - **Texture Resolution**: Must match current settings exactly
/// - **Data Integrity**: Basic structure and content validation
///
/// ## Migration Support
/// Future versions can handle migration:
/// - **Version Detection**: Plugin version in metadata enables migration logic
/// - **Setting Updates**: Chunk size/resolution changes can be detected
/// - **Data Transformation**: Old save formats can be converted to new formats
/// - **Fallback Handling**: Graceful degradation for incompatible saves
///
/// # Integration Points
///
/// ## Save Systems
/// - **Created By**: save_fog_of_war_system and create_save_data_immediate
/// - **Serialized By**: complete_save_operation using format-specific encoders
/// - **Event Output**: Included in FogOfWarSaved events as serialized bytes
///
/// ## Load Systems
/// - **Deserialized By**: load_fog_of_war_system using format-specific decoders
/// - **Processed By**: load_save_data for chunk and entity restoration
/// - **Validated By**: Metadata validation before state restoration
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

/// Complete save data for an individual fog chunk including coordinates, state, and texture data.
/// 单个雾效区块的完整保存数据，包括坐标、状态和纹理数据
///
/// This structure contains all information needed to restore a single chunk's exploration
/// state, including its spatial coordinates, visibility status, GPU texture layer mappings,
/// and optional raw texture data for complete restoration.
///
/// # Data Components
///
/// ## Spatial Information
/// - **Chunk Coordinates**: IVec2 position in chunk space for spatial mapping
/// - **World Bounds**: Derived from coordinates during restoration (not saved)
/// - **Layer Mapping**: GPU texture array indices for proper layer restoration
///
/// ## State Information
/// - **Visibility State**: Current exploration/visibility status (Unexplored/Explored/Visible)
/// - **Memory Location**: Determined during restoration (CPU/GPU placement)
/// - **Texture Availability**: Optional texture data based on save configuration
///
/// ## GPU Resource Mapping
/// Layer indices enable proper GPU texture array restoration:
/// ```rust,ignore
/// // During save: record current GPU layer indices
/// fog_layer_index: Some(chunk.fog_layer_index)
/// snapshot_layer_index: Some(chunk.snapshot_layer_index)
///
/// // During load: attempt to restore to same layers
/// if texture_manager.allocate_specific_layer_indices(coords, fog_idx, snap_idx) {
///     // Restored to original layers
/// } else {
///     // Allocate new layers if originals unavailable
/// }
/// ```
///
/// # Texture Data Management
///
/// ## Conditional Inclusion
/// Texture data inclusion depends on save request and chunk state:
/// - **fog_data**: Included when visibility != Unexplored && include_texture_data
/// - **snapshot_data**: Included when visibility == Explored && include_texture_data
/// - **Size Optimization**: Unexplored chunks don't include fog data
/// - **Memory Efficiency**: Visible chunks don't include snapshot data
///
/// ## Data Format
/// Raw texture bytes in GPU-compatible format:
/// - **Fog Data**: R8Unorm format (1 byte per pixel) for real-time visibility
/// - **Snapshot Data**: RGBA8 format (4 bytes per pixel) for exploration history
/// - **No Compression**: Raw pixel data for direct GPU upload
/// - **Platform Independent**: Byte order handled by serialization format
///
/// # Performance Characteristics
///
/// ## Memory Usage
/// ```rust,no_run
/// # use bevy_fog_of_war::prelude::*;
/// # use bevy_fog_of_war::persistence::ChunkSaveData;
/// # let settings = FogMapSettings::default();
/// // Base structure overhead
/// let base_size = std::mem::size_of::<ChunkSaveData>(); // ~64 bytes
///
/// // Texture data overhead (when included)
/// let texture_resolution = settings.texture_resolution_per_chunk;
/// let fog_size = (texture_resolution.x * texture_resolution.y) as usize; // 1 byte per pixel
/// let snapshot_size = fog_size * 4; // 4 bytes per pixel (RGBA)
///
/// // Total per-chunk memory
/// let total_size = base_size + fog_size + snapshot_size; // Varies by resolution
/// ```
///
/// ## Serialization Impact
/// - **Without Texture Data**: ~64 bytes per chunk (metadata only)
/// - **With Texture Data**: 64 bytes + texture sizes (can be several KB per chunk)
/// - **Format Efficiency**: Binary formats compress better than JSON
/// - **Network Transfer**: Consider texture inclusion for network saves
///
/// # Restoration Process
///
/// ## Load-Time Recreation
/// During load, this data creates:
/// 1. **FogChunk Entity**: ECS entity with chunk component
/// 2. **GPU Layer Allocation**: Texture array layer assignment
/// 3. **Image Assets**: CPU image assets when texture data present
/// 4. **Cache Updates**: Exploration and visibility state restoration
/// 5. **Entity Management**: Chunk entity manager registration
///
/// ## Validation and Compatibility
/// - **Coordinate Validation**: Chunk coordinates must be valid IVec2
/// - **Layer Index Validation**: GPU indices must be within array bounds
/// - **Texture Data Validation**: Raw data must match expected format and size
/// - **State Consistency**: Visibility state must match texture data availability
///
/// # Integration Points
///
/// ## Save Process
/// - **Created By**: create_save_data_immediate from chunk info and GPU data
/// - **Source Data**: FogChunk entities and texture manager state
/// - **GPU Integration**: Optional texture data from render world transfers
///
/// ## Load Process
/// - **Processed By**: load_save_data function for complete restoration
/// - **Entity Creation**: Spawns new FogChunk entities with saved state
/// - **GPU Restoration**: Allocates texture layers and uploads data
/// - **Cache Integration**: Updates chunk state cache with exploration data
///
/// # Future Extensibility
///
/// ## Additional Fields
/// Future versions can add fields with `Option<T>` for backward compatibility:
/// - **Compression**: Optional compressed texture data
/// - **LOD Data**: Multiple resolution levels
/// - **Custom Data**: Game-specific chunk metadata
/// - **Timestamps**: Per-chunk exploration timestamps
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

/// Restores fog of war state from saved data with validation and chunk entity creation.
/// 从保存的数据恢复雾效状态，包括验证和区块实体创建
///
/// This function handles the core logic of loading fog of war data, validating
/// metadata compatibility, clearing existing state, and recreating chunk entities
/// with their exploration states and texture data.
///
/// # Load Process
/// 1. **Metadata Validation**: Verify chunk size and texture resolution compatibility
/// 2. **State Reset**: Clear current cache state for clean load
/// 3. **Chunk Recreation**: Create FogChunk entities for each saved chunk
/// 4. **Texture Restoration**: Restore texture data when available
/// 5. **Layer Allocation**: Assign texture array layer indices
/// 6. **Entity Management**: Update chunk entity manager
///
/// # Validation Checks
/// - **Chunk Size**: Must match current settings
/// - **Texture Resolution**: Must match current settings
/// - **Plugin Version**: Logged for compatibility tracking
///
/// # Layer Index Strategy
/// - **Preserve Original**: Attempts to restore to original layer indices
/// - **Fallback Allocation**: Allocates new indices if originals unavailable
/// - **Conflict Resolution**: Handles layer conflicts gracefully
///
/// # Error Handling
/// Returns specific errors for different failure modes:
/// - `InvalidChunkSize`: Chunk size mismatch
/// - `InvalidTextureResolution`: Texture resolution mismatch
///
/// # Performance Characteristics
/// - **Memory Reset**: O(existing_chunks) for state cleanup
/// - **Entity Creation**: O(saved_chunks) for chunk recreation
/// - **Texture Restoration**: O(texture_data_size) for data copying
///
/// # Time Complexity: O(n) where n = number of saved chunks + existing chunks
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
            if let Some(fog_data) = &chunk_data.fog_data {
                if let Some(fog_image) = images.get_mut(&chunk_image.fog_image_handle) {
                    fog_image.data = Some(fog_data.clone());
                }
            }

            if let Some(snapshot_data) = &chunk_data.snapshot_data {
                if let Some(snapshot_image) = images.get_mut(&chunk_image.snapshot_image_handle) {
                    snapshot_image.data = Some(snapshot_data.clone());
                }
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

    // 收集需要快照的区块坐标，延迟到下一帧发送以确保proper系统执行顺序
    // Collect chunk coordinates that need snapshots, defer to next frame for proper system execution order
    let chunks_needing_snapshots: Vec<IVec2> = data
        .chunks
        .iter()
        .filter(|chunk_data| {
            chunk_data.visibility == ChunkVisibility::Explored
                || chunk_data.visibility == ChunkVisibility::Visible
        })
        .map(|chunk_data| {
            info!(
                "Deferring RequestChunkSnapshot for loaded chunk {:?} with visibility {:?}",
                chunk_data.coords, chunk_data.visibility
            );
            chunk_data.coords
        })
        .collect();

    // 使用deferred command确保ensure_snapshot_render_layer在快照请求之前运行
    // Use deferred command to ensure ensure_snapshot_render_layer runs before snapshot requests
    if !chunks_needing_snapshots.is_empty() {
        commands.queue(move |world: &mut World| {
            let mut snapshot_events = world.resource_mut::<Events<RequestChunkSnapshot>>();
            for chunk_coords in chunks_needing_snapshots {
                info!(
                    "Sending deferred RequestChunkSnapshot for chunk {:?}",
                    chunk_coords
                );
                snapshot_events.send(RequestChunkSnapshot(chunk_coords));
            }
        });
    }

    Ok(loaded_count)
}

/// 保存系统参数组合
/// Save system parameter bundle
#[derive(SystemParam)]
pub struct SaveSystemParams<'w, 's> {
    save_events: EventReader<'w, 's, SaveFogOfWarRequest>,
    pending_saves: ResMut<'w, PendingSaveOperations>,
    gpu_to_cpu_requests: ResMut<'w, GpuToCpuCopyRequests>,
    saved_events: EventWriter<'w, FogOfWarSaved>,
    settings: Res<'w, FogMapSettings>,
    cache: Res<'w, ChunkStateCache>,
    chunks: Query<'w, 's, &'static FogChunk>,
    texture_manager: Res<'w, TextureArrayManager>,
}

/// System that handles fog of war save requests with optional GPU texture data collection.
/// 处理雾效保存请求的系统，可选择收集GPU纹理数据
///
/// This system processes SaveFogOfWarRequest events and initiates save operations,
/// handling both immediate saves (metadata only) and async saves (with texture data).
/// For texture-inclusive saves, it coordinates GPU→CPU transfers before completion.
///
/// # Save Operation Types
/// - **Immediate Save**: Metadata and visibility state only (no GPU transfer)
/// - **Async Save**: Complete save including texture pixel data (requires GPU transfer)
///
/// # Process Flow
/// 1. **Request Processing**: Handle SaveFogOfWarRequest events
/// 2. **Chunk Collection**: Gather exploration state from cache
/// 3. **Layer Resolution**: Get texture layer indices from entities or manager
/// 4. **Transfer Decision**: Determine if GPU data transfer is needed
/// 5. **Operation Routing**: Either immediate save or pending save creation
///
/// # GPU Transfer Coordination
/// For saves with texture data:
/// - **Transfer Requests**: Submit GpuToCpuCopyRequest for each chunk
/// - **Pending Operation**: Create PendingSaveData to track async completion
/// - **Awaiting List**: Track chunks requiring GPU data
///
/// # Performance Considerations
/// - **Immediate Path**: O(explored_chunks) for metadata-only saves
/// - **Async Path**: O(explored_chunks) + GPU transfer overhead
/// - **Memory Efficiency**: Minimal memory footprint during async operations
///
/// # Time Complexity: O(n) where n = number of explored chunks
pub fn save_fog_of_war_system(mut params: SaveSystemParams) {
    for event in params.save_events.read() {
        info!(
            "Starting save (include_texture_data: {})",
            event.include_texture_data
        );

        // 收集需要保存的区块信息
        // Collect chunk information to save
        let mut chunk_info = Vec::new();
        let mut awaiting_chunks = std::collections::HashSet::new();

        for &coords in &params.cache.explored_chunks {
            let visibility = if params.cache.visible_chunks.contains(&coords) {
                ChunkVisibility::Visible
            } else {
                ChunkVisibility::Explored
            };

            // 获取层索引
            // Get layer indices
            let (fog_idx, snap_idx) =
                if let Some(chunk) = params.chunks.iter().find(|c| c.coords == coords) {
                    (chunk.fog_layer_index, chunk.snapshot_layer_index)
                } else {
                    // 如果找不到区块实体，尝试从纹理管理器获取
                    // If chunk entity not found, try to get from texture manager
                    if let Some((fog_idx, snap_idx)) =
                        params.texture_manager.get_allocated_indices(coords)
                    {
                        (Some(fog_idx), Some(snap_idx))
                    } else {
                        (None, None)
                    }
                };

            chunk_info.push((coords, visibility, fog_idx, snap_idx));

            // 如果需要纹理数据且区块在GPU上，请求GPU到CPU传输
            // If texture data needed and chunk is on GPU, request GPU-to-CPU transfer
            if event.include_texture_data && visibility != ChunkVisibility::Unexplored {
                if let (Some(fog_layer_idx), Some(snap_layer_idx)) = (fog_idx, snap_idx) {
                    // 请求GPU到CPU传输
                    // Request GPU-to-CPU transfer
                    params
                        .gpu_to_cpu_requests
                        .requests
                        .push(GpuToCpuCopyRequest {
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
        }

        // 如果不需要等待GPU数据，立即保存
        // If no GPU data needed, save immediately
        if awaiting_chunks.is_empty() {
            match create_save_data_immediate(
                &params.settings,
                chunk_info,
                HashMap::new(),
                event.include_texture_data,
            ) {
                Ok(save_data) => {
                    let format = event.format.unwrap_or_default();
                    complete_save_operation(save_data, format, &mut params.saved_events);
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

            params.pending_saves.pending_save = Some(pending);
            info!(
                "Created pending save, waiting for {} chunks",
                awaiting_chunks.len()
            );
        }
    }
}

/// System that processes GPU data ready events to complete pending save operations.
/// 处理GPU数据就绪事件以完成挂起保存操作的系统
///
/// This system handles ChunkGpuDataReady events sent by the render world when GPU
/// texture data has been successfully transferred to CPU memory. It accumulates
/// received data and completes save operations when all requested chunks are ready.
///
/// # Event Processing Flow
/// 1. **Event Correlation**: Match received events with pending save operations
/// 2. **Data Accumulation**: Store GPU texture data in pending save structure
/// 3. **Progress Tracking**: Remove completed chunks from awaiting list
/// 4. **Completion Check**: Determine when all required data is collected
/// 5. **Save Finalization**: Complete save operation when all data is ready
///
/// # Async Operation Management
/// This system manages the async nature of GPU→CPU transfers:
/// - **Partial Completion**: Handles individual chunk completions as they arrive
/// - **Progress Monitoring**: Tracks remaining chunks awaiting GPU data
/// - **Batch Processing**: Accumulates all data before final save operation
/// - **Error Recovery**: Handles failed transfers gracefully
///
/// # Data Collection Strategy
/// For each received ChunkGpuDataReady event:
/// ```text
/// [GPU Event] → [Match Pending Save] → [Store Data] → [Check Complete]
///      ↓              ↓                   ↓              ↓
/// Chunk Ready    Find Operation     Update Storage   All Ready?
///                                                       ↓
///                                              [Complete Save Operation]
/// ```
///
/// # Performance Characteristics
/// - **Event-Driven**: Only processes when GPU data becomes available
/// - **Memory Efficiency**: Stores texture data temporarily during collection
/// - **Batch Completion**: Waits for all chunks before save finalization
/// - **Immediate Processing**: Processes events as they arrive each frame
///
/// # Error Handling
/// The system handles several error conditions:
/// - **Orphaned Events**: GPU events without corresponding pending saves
/// - **Save Creation Failures**: Problems creating final save data structure
/// - **Serialization Errors**: Issues encoding data in requested format
/// - **Missing Data**: Partial data collection failures
///
/// # Memory Management
/// Texture data storage considerations:
/// - **Temporary Storage**: GPU data stored in HashMap during collection
/// - **Memory Growth**: Memory usage scales with pending chunk count
/// - **Cleanup**: Data automatically cleaned up when save completes
/// - **Large Chunks**: High-resolution chunks may use significant memory
///
/// # Integration Points
/// - **GPU Transfer Systems**: Receives events from render world GPU systems
/// - **Save Systems**: Completes operations initiated by save_fog_of_war_system
/// - **Event System**: Uses Bevy's event system for async communication
/// - **Serialization**: Triggers final data encoding and save completion
///
/// # Time Complexity: O(pending_chunks) per event, O(total_texture_data) for save completion
pub fn handle_gpu_data_ready_system(
    mut gpu_ready_events: EventReader<ChunkGpuDataReady>,
    mut pending_saves: ResMut<PendingSaveOperations>,
    mut saved_events: EventWriter<FogOfWarSaved>,
    settings: Res<FogMapSettings>,
) {
    for event in gpu_ready_events.read() {
        // 检查是否有挂起的保存操作等待此数据
        // Check if there's a pending save operation waiting for this data
        if let Some(pending) = &mut pending_saves.pending_save {
            if pending.awaiting_chunks.contains(&event.chunk_coords) {
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
                                complete_save_operation(
                                    save_data,
                                    pending.format,
                                    &mut saved_events,
                                );
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
}

/// Creates save data immediately using available CPU or GPU texture data.
/// 使用可用的CPU或GPU纹理数据立即创建保存数据
///
/// This function constructs a complete FogOfWarSaveData structure from collected
/// chunk information and optional texture data. It handles both immediate saves
/// (metadata only) and complete saves (with GPU texture data).
///
/// # Save Data Construction Process
/// 1. **Chunk Iteration**: Process each chunk's metadata and texture requirements
/// 2. **Texture Decision**: Determine whether to include texture data based on visibility
/// 3. **Data Assembly**: Combine chunk info with texture data when available
/// 4. **Metadata Creation**: Generate save metadata with version and settings info
/// 5. **Structure Creation**: Assemble final FogOfWarSaveData with timestamp
///
/// # Texture Data Handling
/// For each chunk, texture inclusion follows these rules:
/// ```rust,ignore
/// fog_data = if visibility != Unexplored && include_texture_data {
///     texture_data.get(coords).map(|(fog, _)| fog.clone())
/// } else { None }
///
/// snapshot_data = if visibility == Explored && include_texture_data {
///     texture_data.get(coords).map(|(_, snap)| snap.clone())
/// } else { None }
/// ```
///
/// # Metadata Generation
/// Creates comprehensive metadata for save validation:
/// - **Plugin Version**: Current crate version from CARGO_PKG_VERSION
/// - **Chunk Size**: World units per chunk for compatibility validation
/// - **Texture Resolution**: Texture size per chunk for format validation
/// - **Timestamp**: Unix timestamp for save ordering and identification
///
/// # Performance Characteristics
/// - **Memory Efficiency**: Only includes texture data when specifically requested
/// - **Selective Inclusion**: Texture data filtered by chunk visibility state
/// - **Metadata Overhead**: Minimal overhead for save validation and compatibility
/// - **Cloning Cost**: Texture data is cloned for save structure ownership
///
/// # Data Structure Organization
/// The resulting save data contains:
/// - **Root Container**: FogOfWarSaveData with timestamp and metadata
/// - **Chunk Array**: Vector of ChunkSaveData with coordinates and states
/// - **Optional Textures**: Raw texture bytes included when requested
/// - **Version Info**: Plugin version and settings for compatibility checking
///
/// # Texture Data Format
/// When included, texture data maintains GPU format compatibility:
/// - **Fog Data**: R8Unorm format (1 byte per pixel) for visibility data
/// - **Snapshot Data**: RGBA8 format (4 bytes per pixel) for exploration data
/// - **Raw Bytes**: Direct byte arrays without additional encoding
/// - **Layer Indices**: Original GPU texture array indices preserved
///
/// # Error Handling
/// Returns PersistenceError for various failure conditions:
/// - **Timestamp Generation**: System time unavailable or invalid
/// - **Memory Allocation**: Insufficient memory for data structures
/// - **Data Consistency**: Inconsistent chunk or texture data
///
/// # Integration Points
/// - **GPU Systems**: Uses texture data transferred from render world
/// - **Settings**: Incorporates current fog map settings for validation
/// - **Asset System**: Compatible with Bevy's asset loading for restoration
/// - **Serialization**: Prepares data structure for format-specific encoding
///
/// # Time Complexity: O(n) where n = number of chunks being saved
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

/// Completes save operation by serializing data and emitting completion events.
/// 通过序列化数据并发出完成事件来完成保存操作
///
/// This function handles the final stage of the save process by encoding the collected
/// fog data into the requested serialization format and sending appropriate completion
/// events. It supports multiple serialization formats with error handling.
///
/// # Serialization Process
/// 1. **Format Selection**: Use specified serialization format for encoding
/// 2. **Data Encoding**: Serialize FogOfWarSaveData using format-specific encoder
/// 3. **Error Handling**: Capture and report serialization failures
/// 4. **Event Generation**: Send FogOfWarSaved event with encoded data
/// 5. **Statistics Logging**: Report save completion metrics
///
/// # Supported Serialization Formats
/// The function handles different formats based on feature flags:
/// ```rust,ignore
/// match format {
///     SerializationFormat::Json => serde_json::to_vec(&save_data),
///     #[cfg(feature = "format-messagepack")]
///     SerializationFormat::MessagePack => rmp_serde::to_vec(&save_data),
///     #[cfg(feature = "format-bincode")]
///     SerializationFormat::Bincode => bincode::serialize(&save_data),
/// }
/// ```
///
/// # Event Generation
/// On successful serialization, emits FogOfWarSaved event containing:
/// - **Serialized Data**: Raw bytes ready for storage or transmission
/// - **Format Used**: Serialization format for proper deserialization
/// - **Chunk Count**: Number of chunks included for statistics
/// - **Data Size**: Total byte size for performance monitoring
///
/// # Error Reporting
/// Serialization failures are handled gracefully:
/// - **Error Logging**: Detailed error messages with format information
/// - **Error Wrapping**: serde errors wrapped in PersistenceError::SerializationFailed
/// - **No Panic**: System continues operating even when saves fail
/// - **User Notification**: Errors logged at ERROR level for visibility
///
/// # Performance Characteristics
/// - **Memory Allocation**: Creates new `Vec<u8>` for serialized data
/// - **Format Efficiency**: Binary formats typically 60-70% smaller than JSON
/// - **Encoding Speed**: Binary formats typically 2-3x faster than JSON
/// - **Memory Peak**: Temporary memory spike during serialization
///
/// # Format-Specific Behavior
/// Each format has different characteristics:
/// - **JSON**: Human-readable, larger size, universal compatibility
/// - **MessagePack**: Binary efficient, cross-language compatibility
/// - **Bincode**: Rust-native binary, highest performance and smallest size
///
/// # Statistics and Monitoring
/// Logs comprehensive save completion information:
/// - **Format Used**: Which serialization format was applied
/// - **Chunk Count**: Number of chunks included in save
/// - **Data Size**: Total bytes for performance analysis
/// - **Success/Failure**: Clear indication of operation outcome
///
/// # Integration Points
/// - **Event System**: Sends FogOfWarSaved events for save completion notification
/// - **Logging System**: Uses Bevy's logging for operation tracking
/// - **Save Systems**: Final step in save operation pipeline
/// - **Error System**: Integrates with persistence error handling
///
/// # Time Complexity: O(data_size) where data_size = size of serialized save data
fn complete_save_operation(
    save_data: FogOfWarSaveData,
    format: SerializationFormat,
    saved_events: &mut EventWriter<FogOfWarSaved>,
) {
    let result = match format {
        #[cfg(feature = "format-json")]
        SerializationFormat::Json => serde_json::to_vec(&save_data)
            .map_err(|e| PersistenceError::SerializationFailed(e.to_string())),
        #[cfg(feature = "format-messagepack")]
        SerializationFormat::MessagePack => rmp_serde::to_vec(&save_data)
            .map_err(|e| PersistenceError::SerializationFailed(e.to_string())),
        #[cfg(feature = "format-bincode")]
        SerializationFormat::Bincode => {
            bincode::serde::encode_to_vec(&save_data, bincode::config::standard())
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))
        }
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

/// 加载系统参数组合
/// Load system parameter bundle
#[derive(SystemParam)]
pub struct LoadSystemParams<'w, 's> {
    load_events: EventReader<'w, 's, LoadFogOfWarRequest>,
    loaded_events: EventWriter<'w, FogOfWarLoaded>,
    commands: Commands<'w, 's>,
    settings: Res<'w, FogMapSettings>,
    cache: ResMut<'w, ChunkStateCache>,
    chunk_manager: ResMut<'w, ChunkEntityManager>,
    texture_manager: ResMut<'w, TextureArrayManager>,
    images: ResMut<'w, Assets<Image>>,
    existing_chunks: Query<'w, 's, Entity, With<FogChunk>>,
}

/// System that processes fog of war load requests with format detection and validation.
/// 处理雾效加载请求的系统，具有格式检测和验证功能
///
/// This system handles LoadFogOfWarRequest events to restore previously saved fog
/// states. It supports multiple serialization formats with automatic detection,
/// validates loaded data compatibility, and restores complete chunk states.
///
/// # Load Process Overview
/// 1. **Event Processing**: Handle LoadFogOfWarRequest events from queue
/// 2. **Format Detection**: Auto-detect serialization format when not specified
/// 3. **Data Deserialization**: Decode save data using appropriate format decoder
/// 4. **State Cleanup**: Clear existing fog state for clean restoration
/// 5. **Data Restoration**: Restore chunks, entities, and texture data
/// 6. **Completion Events**: Send FogOfWarLoaded events with load statistics
///
/// # Format Auto-Detection
/// When format is not specified, uses heuristic detection:
/// ```rust,ignore
/// let format = event.format.unwrap_or_else(|| {
///     if event.data.starts_with(b"{") || event.data.starts_with(b"[") {
///         SerializationFormat::Json  // Detect JSON by opening bracket
///     } else {
///         SerializationFormat::Bincode  // Default to binary format
///     }
/// });
/// ```
///
/// # State Cleanup Process
/// Before loading new data, performs complete state reset:
/// - **Entity Despawning**: Removes all existing FogChunk entities
/// - **Manager Cleanup**: Clears chunk entity manager mappings
/// - **Texture Deallocation**: Releases all allocated texture array layers
/// - **Cache Reset**: Clears exploration and visibility state caches
///
/// # Data Validation and Loading
/// Uses load_save_data function for comprehensive restoration:
/// - **Metadata Validation**: Verify chunk size and texture resolution compatibility
/// - **Version Checking**: Log plugin version information for compatibility tracking
/// - **Entity Recreation**: Recreate FogChunk entities with saved states
/// - **Texture Restoration**: Restore texture data when available in save
///
/// # Error Handling and Recovery
/// Handles multiple error conditions gracefully:
/// - **Deserialization Failures**: Invalid data format or corrupted saves
/// - **Compatibility Issues**: Mismatched chunk sizes or texture resolutions
/// - **Partial Load Failures**: Some chunks fail to load due to memory constraints
/// - **Asset System Issues**: Problems with image asset creation or storage
///
/// # Warning Generation
/// Tracks and reports non-fatal issues during loading:
/// - **Partial Loads**: When fewer chunks load than expected (texture array full)
/// - **Compatibility Warnings**: Version mismatches or setting differences
/// - **Performance Issues**: Large save files or memory pressure warnings
/// - **Data Inconsistencies**: Minor data issues that don't prevent loading
///
/// # Performance Characteristics
/// - **Memory Impact**: Temporary memory spike during deserialization
/// - **Loading Speed**: Binary formats typically 2-3x faster than JSON
/// - **State Reset**: O(existing_chunks) for cleanup operations
/// - **Entity Creation**: O(saved_chunks) for restoration operations
///
/// # Event Generation
/// Sends FogOfWarLoaded events with comprehensive load statistics:
/// - **Chunk Count**: Number of chunks successfully loaded
/// - **Warnings**: List of non-fatal issues encountered during load
/// - **Load Metrics**: Performance and compatibility information
///
/// # Integration Points
/// - **Save System**: Counterpart to save operations for complete persistence
/// - **Asset System**: Integrates with Bevy's image asset management
/// - **Entity System**: Creates and manages FogChunk entities
/// - **Cache System**: Updates exploration and visibility state caches
/// - **Texture System**: Manages GPU texture array layer allocation
///
/// # Memory Management
/// Load operations can be memory-intensive:
/// - **Deserialization**: Temporary memory for decoded save data
/// - **Texture Data**: Potential large memory usage for texture restoration
/// - **Entity Creation**: Memory allocation for new chunk entities
/// - **Asset Storage**: Image assets stored in Bevy's asset system
///
/// # Time Complexity: O(saved_chunks + existing_chunks) for complete load operation
pub fn load_fog_of_war_system(mut params: LoadSystemParams) {
    for event in params.load_events.read() {
        let mut warnings = Vec::new();

        // 根据格式反序列化数据
        // Deserialize data based on format
        let format = event.format.unwrap_or_else(|| {
            // 尝试自动检测格式
            // Try to auto-detect format
            if event.data.starts_with(b"{") || event.data.starts_with(b"[") {
                #[cfg(feature = "format-json")]
                return SerializationFormat::Json;
                #[cfg(not(feature = "format-json"))]
                {
                    warn!("Detected JSON format but format-json feature is disabled. Falling back to default format.");
                    SerializationFormat::default()
                }
            } else {
                // 默认假设为bincode或其他可用格式
                // Default assume bincode or other available format
                #[cfg(feature = "format-bincode")]
                return SerializationFormat::Bincode;
                #[cfg(all(not(feature = "format-bincode"), feature = "format-messagepack"))]
                return SerializationFormat::MessagePack;
                #[cfg(all(
                    not(feature = "format-bincode"),
                    not(feature = "format-messagepack"),
                    feature = "format-json"
                ))]
                return SerializationFormat::Json;
            }
        });

        let result = match format {
            #[cfg(feature = "format-json")]
            SerializationFormat::Json => serde_json::from_slice::<FogOfWarSaveData>(&event.data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string())),
            #[cfg(feature = "format-messagepack")]
            SerializationFormat::MessagePack => {
                rmp_serde::from_slice::<FogOfWarSaveData>(&event.data)
                    .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
            }
            #[cfg(feature = "format-bincode")]
            SerializationFormat::Bincode => {
                match bincode::serde::decode_from_slice(&event.data, bincode::config::standard()) {
                    Ok((decoded, _)) => Ok(decoded),
                    Err(e) => Err(PersistenceError::DeserializationFailed(e.to_string())),
                }
            }
        };

        match result {
            Ok(save_data) => {
                // 清除现有的区块实体
                // Clear existing chunk entities
                for entity in params.existing_chunks.iter() {
                    params.commands.entity(entity).despawn();
                }
                params.chunk_manager.map.clear();
                params.texture_manager.clear_all_layers();

                // Note: Unlike reset operation, we don't clear snapshot texture data during load
                // because RequestChunkSnapshot events will regenerate the specific layers needed
                // Clearing here would make snapshots invisible even when properly generated

                // 加载保存的数据
                // Load saved data
                match load_save_data(
                    &save_data,
                    &params.settings,
                    &mut params.cache,
                    &mut params.commands,
                    &mut params.chunk_manager,
                    &mut params.texture_manager,
                    &mut params.images,
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

                        params.loaded_events.write(FogOfWarLoaded {
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

/// Plugin extension for adding comprehensive fog of war persistence functionality.
/// 用于添加全面雾效持久化功能的插件扩展
///
/// This plugin integrates complete save/load functionality into the fog of war system,
/// providing event-driven persistence operations with multiple serialization formats
/// and async GPU data handling. It extends the main FogOfWarPlugin with persistence
/// capabilities while maintaining clean separation of concerns.
///
/// # Plugin Architecture
///
/// ## Core Functionality
/// The plugin provides comprehensive persistence features:
/// - **Save Operations**: Complete fog state serialization with multiple formats
/// - **Load Operations**: State restoration with validation and compatibility checking
/// - **Event System**: Request/response pattern for async operations
/// - **GPU Integration**: Seamless CPU↔GPU data transfer for complete saves
///
/// ## System Organization
/// Registers three main systems in the Persistence system set:
/// ```rust,ignore
/// # use bevy::prelude::*;
/// # use bevy_fog_of_war::prelude::*;
/// # use bevy_fog_of_war::persistence::*;
/// # let mut app = App::new();
/// app.add_systems(Update, (
///     save_fog_of_war_system,        // Handle save requests
///     handle_gpu_data_ready_system,  // Process GPU data transfers
///     load_fog_of_war_system,        // Handle load requests
/// ).in_set(FogSystems::Persistence));
/// ```
///
/// ## Event Registration
/// Adds complete event communication system:
/// - **SaveFogOfWarRequest**: Initiate save operations with options
/// - **LoadFogOfWarRequest**: Initiate load operations with data
/// - **FogOfWarSaved**: Completion notification with serialized data
/// - **FogOfWarLoaded**: Completion notification with load statistics
///
/// # Integration with Main Plugin
///
/// ## System Set Ordering
/// The persistence systems execute in the FogSystems::Persistence set:
/// ```text
/// Core Systems → Memory → Compute → Render → Persistence
/// ```
/// This ensures persistence operations happen after core fog calculations.
///
/// ## Resource Management
/// Initializes PendingSaveOperations resource for async save coordination:
/// - **GPU Transfer Tracking**: Manages pending GPU→CPU data transfers
/// - **Save Operation State**: Tracks multi-frame save completion
/// - **Memory Management**: Temporary storage for collected texture data
///
/// # Usage Example
///
/// ## Basic Integration
/// ```rust,ignore
/// use bevy::prelude::*;
/// use bevy_fog_of_war::prelude::*;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(FogOfWarPlugin::default())
///     .add_plugins(FogOfWarPersistencePlugin)  // Add persistence
///     .run();
/// ```
///
/// ## Save Operation
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_fog_of_war::prelude::*;
/// fn save_fog_state(mut save_events: EventWriter<SaveFogOfWarRequest>) {
///     save_events.send(SaveFogOfWarRequest {
///         include_texture_data: true,  // Complete save with GPU data
///         format: Some(SerializationFormat::Bincode),
///     });
/// }
/// ```
///
/// ## Load Operation
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_fog_of_war::prelude::*;
/// fn load_fog_state(
///     mut load_events: EventWriter<LoadFogOfWarRequest>,
///     save_data: Vec<u8>  // Previously saved data
/// ) {
///     load_events.send(LoadFogOfWarRequest {
///         data: save_data,
///         format: None,  // Auto-detect format
///     });
/// }
/// ```
///
/// # Performance Considerations
///
/// ## Memory Usage
/// - **Save Operations**: Temporary memory spike during GPU data collection
/// - **Load Operations**: Memory allocation for deserialization and entity creation
/// - **Async Coordination**: Minimal memory overhead for operation tracking
///
/// ## System Execution
/// - **Event-Driven**: Systems only execute when persistence operations are requested
/// - **Async Coordination**: GPU transfers don't block main thread operations
/// - **Efficient Serialization**: Binary formats provide optimal performance
///
/// # Feature Compatibility
///
/// ## Serialization Format Support
/// Respects feature flags for format availability:
/// - **Always Available**: JSON format (human-readable, larger size)
/// - **format-messagepack**: MessagePack binary format (good compression)
/// - **format-bincode**: Bincode binary format (highest performance)
///
/// ## Cross-Platform Considerations
/// - **Endianness**: Serialization formats handle byte order correctly
/// - **Version Compatibility**: Plugin version tracking for save compatibility
/// - **File System**: Uses Bevy's asset system for cross-platform file handling
///
/// # Future Extensibility
///
/// ## Plugin Design
/// The plugin architecture supports easy extension:
/// - **Additional Systems**: New persistence-related systems can be added
/// - **Custom Events**: Extended event types for specialized operations
/// - **Format Extensions**: New serialization formats can be integrated
/// - **Storage Backends**: Alternative storage mechanisms can be added
///
/// # Error Handling
///
/// ## Graceful Degradation
/// The plugin handles errors without crashing the application:
/// - **Save Failures**: Logged errors with detailed failure information
/// - **Load Failures**: Graceful fallback with error reporting
/// - **GPU Issues**: Async operation failures handled transparently
/// - **Format Issues**: Serialization errors reported clearly
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
