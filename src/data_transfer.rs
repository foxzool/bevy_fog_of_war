use crate::prelude::*;
use bevy::asset::Handle;
use bevy::ecs::event::Event;
use bevy::math::IVec2;
use bevy::prelude::Image;
use bevy::prelude::Resource;
use bevy::reflect::Reflect;
use bevy::render::extract_resource::ExtractResource;

/// Resource for coordinating GPU-to-CPU texture data transfers between main and render worlds.
/// 主世界与渲染世界之间协调 GPU 到 CPU 纹理数据传输的资源
///
/// This resource serves as a communication channel for the main world to request that
/// the render world copy specific chunk texture data from GPU memory to CPU memory.
/// It's part of the cross-world synchronization system that enables dynamic memory
/// management for fog of war chunks.
///
/// # Architecture
/// - **Main World**: Populates this resource with transfer requests
/// - **Render World**: Processes requests and copies GPU texture data to staging buffers
/// - **Async Communication**: Uses events to signal transfer completion
/// - **Extract Resource**: Automatically synchronized between worlds via Bevy's extract system
///
/// # Use Cases
/// - **Memory Pressure**: Move chunks from GPU to CPU when GPU memory is limited
/// - **Distance Culling**: Transfer chunks that are far from camera to CPU storage
/// - **Persistence**: Copy GPU data to CPU for saving to disk
/// - **Debug/Inspection**: Access GPU texture data for debugging or analysis
///
/// # Transfer Flow
/// ```text
/// [Main World] → Fill GpuToCpuCopyRequests → [Extract] → [Render World]
///      ↑                                                        ↓
/// ChunkGpuDataReady ← [Event] ← GPU texture copy ← Process requests
/// ```
///
/// # Performance Considerations
/// - **Batch Operations**: Multiple requests are processed together for efficiency
/// - **Async Processing**: Transfers happen asynchronously to avoid blocking
/// - **Memory Allocation**: Staging buffers may require temporary memory allocation
/// - **GPU Synchronization**: May require GPU fence synchronization for completion
///
/// # Example Usage
/// ```rust
/// # use bevy_fog_of_war::prelude::*;
/// # use bevy::prelude::*;
/// fn request_chunk_download(
///     mut copy_requests: ResMut<GpuToCpuCopyRequests>,
///     manager: Res<TextureArrayManager>,
/// ) {
///     let chunk_coord = IVec2::new(5, -3);
///     
///     if let Some((fog_idx, snap_idx)) = manager.get_allocated_indices(chunk_coord) {
///         copy_requests.requests.push(GpuToCpuCopyRequest {
///             chunk_coords: chunk_coord,
///             fog_layer_index: fog_idx,
///             snapshot_layer_index: snap_idx,
///         });
///     }
/// }
/// ```
#[derive(Resource, Default, Debug, Clone, Reflect, ExtractResource)]
#[reflect(Resource, Default)]
pub struct GpuToCpuCopyRequests {
    /// Vector of pending GPU-to-CPU transfer requests.
    /// 待处理的 GPU 到 CPU 传输请求向量
    ///
    /// Each request represents a chunk that needs its texture data copied from GPU
    /// texture arrays to CPU memory. The render world processes these requests
    /// and sends ChunkGpuDataReady events when transfers complete.
    ///
    /// **Processing**: Render world drains this vector each frame
    /// **Capacity**: Vector grows as needed, consider pre-allocating for performance
    /// **Ordering**: Requests are processed in FIFO order
    pub requests: Vec<GpuToCpuCopyRequest>,
}

/// Individual request to copy a specific chunk's texture data from GPU to CPU memory.
/// 将特定区块的纹理数据从 GPU 复制到 CPU 内存的单个请求
///
/// This struct represents a single chunk transfer operation containing all the information
/// needed by the render world to locate the chunk's GPU texture data and copy it to
/// CPU-accessible staging buffers.
///
/// # Transfer Process
/// 1. **Locate GPU Data**: Uses layer indices to find chunk textures in GPU arrays
/// 2. **Copy to Staging**: Copies texture data to CPU-readable staging buffers
/// 3. **Extract Data**: Reads pixel data from staging buffers
/// 4. **Send Event**: Sends ChunkGpuDataReady event with extracted data
///
/// # Memory Layout
/// The chunk's data exists in two separate GPU texture arrays:
/// - **Fog Texture**: Real-time visibility data (typically R8Unorm format)
/// - **Snapshot Texture**: Historical exploration data (typically RGBA8 format)
///
/// # Performance Characteristics
/// - **GPU→CPU Transfer**: Relatively expensive operation requiring GPU synchronization
/// - **Staging Buffers**: May require temporary memory allocation
/// - **Async Processing**: Transfer completes asynchronously, signaled by event
/// - **Batch Efficiency**: Multiple requests can be batched for better performance
#[derive(Debug, Clone, Reflect)]
pub struct GpuToCpuCopyRequest {
    /// World coordinates of the chunk to transfer from GPU to CPU.
    /// 要从 GPU 传输到 CPU 的区块的世界坐标
    ///
    /// These coordinates identify which chunk's texture data should be copied.
    /// Used for tracking and event correlation when the transfer completes.
    pub chunk_coords: IVec2,

    /// GPU texture array layer index for the chunk's fog data.
    /// 区块雾效数据的 GPU 纹理数组层索引
    ///
    /// Specifies which layer in the fog texture array contains this chunk's
    /// real-time visibility data. Must be a valid index within the array bounds.
    pub fog_layer_index: u32,

    /// GPU texture array layer index for the chunk's snapshot data.
    /// 区块快照数据的 GPU 纹理数组层索引
    ///
    /// Specifies which layer in the snapshot texture array contains this chunk's
    /// persistent exploration data. Must be a valid index within the array bounds.
    pub snapshot_layer_index: u32,
    // Staging buffer index or some identifier if RenderApp uses a pool
    // 如果 RenderApp 使用池，则为暂存缓冲区索引或某种标识符
}
/// Resource for coordinating CPU-to-GPU texture data uploads between main and render worlds.
/// 主世界与渲染世界之间协调 CPU 到 GPU 纹理数据上传的资源
///
/// This resource serves as a communication channel for the main world to request that
/// the render world upload chunk texture data from CPU memory to GPU texture arrays.
/// It's the reverse operation of GpuToCpuCopyRequests, used when bringing chunks back
/// to GPU memory for active rendering.
///
/// # Architecture
/// - **Main World**: Populates requests with CPU image handles and target GPU layer indices
/// - **Render World**: Processes requests and uploads image data to GPU texture arrays
/// - **Asset System**: Uses Bevy's Image asset handles to access CPU texture data
/// - **Extract Resource**: Automatically synchronized between worlds
///
/// # Use Cases
/// - **GPU Memory Recovery**: Move chunks from CPU back to GPU when memory becomes available
/// - **Camera Approach**: Upload chunks when camera moves closer to them
/// - **Persistence Loading**: Upload chunks from loaded save data
/// - **LOD Management**: Upload higher detail chunks when needed
///
/// # Upload Flow
/// ```text
/// [CPU Image Assets] → CpuToGpuCopyRequests → [Extract] → [Render World]
///      ↑                       ↓                              ↓
/// Asset System         Main World Request                GPU Upload
///                             ↓                              ↓
///                    ChunkCpuDataUploaded ← [Event] ← Upload Complete
/// ```
///
/// # Performance Considerations
/// - **Asset Loading**: CPU image data must be loaded in asset system
/// - **GPU Upload**: Relatively fast operation compared to GPU→CPU transfers
/// - **Memory Allocation**: GPU texture array must have available layer slots
/// - **Batch Processing**: Multiple uploads can be batched for efficiency
#[derive(Resource, Default, Debug, Clone, Reflect, ExtractResource)]
#[reflect(Resource, Default)]
pub struct CpuToGpuCopyRequests {
    /// Vector of pending CPU-to-GPU upload requests.
    /// 待处理的 CPU 到 GPU 上传请求向量
    ///
    /// Each request contains image handles and target GPU layer information.
    /// The render world processes these requests by uploading image data to
    /// the specified texture array layers.
    ///
    /// **Processing**: Render world drains this vector each frame
    /// **Asset Dependencies**: Requires valid Image asset handles
    /// **GPU Allocation**: Target layer indices must be available
    pub requests: Vec<CpuToGpuCopyRequest>,
}

/// Individual request to upload a specific chunk's texture data from CPU to GPU memory.
/// 将特定区块的纹理数据从 CPU 上传到 GPU 内存的单个请求
///
/// This struct represents a single chunk upload operation containing CPU image asset
/// handles and the target GPU texture array layer indices where the data should be uploaded.
///
/// # Upload Process
/// 1. **Resolve Assets**: Access CPU image data via asset handles
/// 2. **Validate Layers**: Ensure target GPU layers are available
/// 3. **Upload Data**: Copy image data to GPU texture array layers
/// 4. **Update Allocation**: Mark layers as allocated in texture manager
/// 5. **Send Event**: Send ChunkCpuDataUploaded completion event
///
/// # Image Asset Requirements
/// - **Fog Image**: Must match fog texture format (typically R8Unorm)
/// - **Snapshot Image**: Must match snapshot texture format (typically RGBA8)
/// - **Dimensions**: Must match chunk texture resolution settings
/// - **Loaded State**: Assets must be fully loaded before upload
///
/// # GPU Memory Management
/// The target layer indices should be reserved/allocated before the upload request:
/// - Either allocated via TextureArrayManager::allocate_layer_indices()
/// - Or specifically allocated via allocate_specific_layer_indices() for persistence
#[derive(Debug, Clone, Reflect)]
pub struct CpuToGpuCopyRequest {
    /// World coordinates of the chunk being uploaded to GPU.
    /// 正在上传到 GPU 的区块的世界坐标
    ///
    /// Used for tracking the upload operation and correlating with completion events.
    pub chunk_coords: IVec2,

    /// Target GPU texture array layer index for fog data.
    /// 雾效数据的目标 GPU 纹理数组层索引
    ///
    /// Specifies which layer in the fog texture array should receive the uploaded
    /// fog data. This layer should be allocated via TextureArrayManager.
    pub fog_layer_index: u32,

    /// Target GPU texture array layer index for snapshot data.
    /// 快照数据的目标 GPU 纹理数组层索引
    ///
    /// Specifies which layer in the snapshot texture array should receive the
    /// uploaded snapshot data. This layer should be allocated via TextureArrayManager.
    pub snapshot_layer_index: u32,

    /// Handle to the CPU image asset containing fog texture data.
    /// 包含雾效纹理数据的 CPU 图像资源句柄
    ///
    /// Must reference a valid Image asset in Bevy's asset system containing
    /// the chunk's fog data in the correct format and dimensions.
    pub fog_image_handle: Handle<Image>,

    /// Handle to the CPU image asset containing snapshot texture data.
    /// 包含快照纹理数据的 CPU 图像资源句柄
    ///
    /// Must reference a valid Image asset in Bevy's asset system containing
    /// the chunk's snapshot data in the correct format and dimensions.
    pub snapshot_image_handle: Handle<Image>,
}

/// Event sent when GPU texture data has been successfully copied to CPU memory.
/// 当 GPU 纹理数据成功复制到 CPU 内存时发送的事件
///
/// This event is sent by the render world to notify the main world that a requested
/// GPU-to-CPU transfer has completed and the texture data is now available in CPU memory.
/// It contains the actual pixel data that was copied from the GPU texture arrays.
///
/// # Event Flow
/// ```text
/// Main World: Request transfer via GpuToCpuCopyRequests
///      ↓
/// Render World: Process transfer, copy GPU → staging buffers → CPU
///      ↓
/// Render World: Send ChunkGpuDataReady event with pixel data
///      ↓
/// Main World: Receive event and handle CPU texture data
/// ```
///
/// # Data Format
/// The pixel data format matches the GPU texture formats:
/// - **Fog Data**: Typically R8Unorm (1 byte per pixel)
/// - **Snapshot Data**: Typically RGBA8 (4 bytes per pixel)
/// - **Layout**: Row-major order, may include padding for alignment
///
/// # Memory Ownership
/// The Vec<u8> data is owned by the event and transferred to the receiving system.
/// Consider the memory cost when processing many chunks simultaneously.
///
/// # Usage Example
/// ```rust
/// # use bevy_fog_of_war::prelude::*;
/// # use bevy::prelude::*;
/// fn handle_gpu_data_ready(
///     mut events: EventReader<ChunkGpuDataReady>,
///     mut images: ResMut<Assets<Image>>,
/// ) {
///     for event in events.read() {
///         println!("Received GPU data for chunk {:?}", event.chunk_coords);
///         
///         // Create CPU image assets from the pixel data
///         let fog_image = create_image_from_data(&event.fog_data, ImageFormat::R8);
///         let snapshot_image = create_image_from_data(&event.snapshot_data, ImageFormat::RGBA8);
///         
///         // Store in asset system or process as needed
///         let fog_handle = images.add(fog_image);
///         let snapshot_handle = images.add(snapshot_image);
///     }
/// }
/// ```
#[derive(Event, Debug)]
pub struct ChunkGpuDataReady {
    /// World coordinates of the chunk whose data was transferred.
    /// 数据已传输的区块的世界坐标
    ///
    /// Used to correlate this event with the original transfer request
    /// and identify which chunk this texture data belongs to.
    pub chunk_coords: IVec2,

    /// Raw pixel data from the chunk's fog texture layer.
    /// 区块雾效纹理层的原始像素数据
    ///
    /// Contains the fog visibility data in the same format as the GPU texture
    /// (typically R8Unorm). Data is in row-major order and may include padding.
    pub fog_data: Vec<u8>,

    /// Raw pixel data from the chunk's snapshot texture layer.
    /// 区块快照纹理层的原始像素数据
    ///
    /// Contains the historical exploration data in the same format as the GPU texture
    /// (typically RGBA8). Data is in row-major order and may include padding.
    pub snapshot_data: Vec<u8>,
}

/// Event sent when CPU texture data has been successfully uploaded to GPU memory.
/// 当 CPU 纹理数据成功上传到 GPU 内存时发送的事件
///
/// This event is sent by the render world to notify the main world that a requested
/// CPU-to-GPU upload has completed and the texture data is now available in GPU
/// texture arrays for rendering operations.
///
/// # Event Flow
/// ```text
/// Main World: Request upload via CpuToGpuCopyRequests (with Image handles)
///      ↓
/// Render World: Load image data from assets, upload to GPU texture arrays
///      ↓
/// Render World: Send ChunkCpuDataUploaded event
///      ↓
/// Main World: Update tracking, free CPU memory if needed
/// ```
///
/// # Usage Example
/// ```rust
/// # use bevy_fog_of_war::prelude::*;
/// # use bevy::prelude::*;
/// fn handle_cpu_data_uploaded(
///     mut events: EventReader<ChunkCpuDataUploaded>,
///     mut cache: ResMut<ChunkStateCache>,
/// ) {
///     for event in events.read() {
///         println!("Chunk {:?} successfully uploaded to GPU", event.chunk_coords);
///         
///         // Update tracking to reflect chunk is now GPU-resident
///         cache.gpu_resident_chunks.insert(event.chunk_coords);
///         
///         // Optionally clean up CPU resources
///         // cleanup_cpu_images_for_chunk(event.chunk_coords);
///     }
/// }
/// ```
#[derive(Event, Debug)]
pub struct ChunkCpuDataUploaded {
    /// World coordinates of the chunk that was uploaded to GPU.
    /// 已上传到 GPU 的区块的世界坐标
    ///
    /// Used to correlate this event with the original upload request
    /// and track which chunk is now available on GPU.
    pub chunk_coords: IVec2,
}

/// 雾效重置错误类型
/// Fog of war reset error types
#[derive(Debug, Clone, PartialEq)]
pub enum FogResetError {
    /// 缓存重置失败
    /// Cache reset failed
    CacheResetFailed(String),
    /// 区块状态重置失败
    /// Chunk state reset failed
    ChunkStateResetFailed(String),
    /// 图像重置失败
    /// Image reset failed
    ImageResetFailed(String),
    /// 纹理重置失败
    /// Texture reset failed
    TextureResetFailed(String),
    /// 实体清理失败
    /// Entity cleanup failed
    EntityCleanupFailed(String),
    /// 渲染世界处理失败
    /// Render world processing failed
    RenderWorldFailed(String),
    /// 回滚失败
    /// Rollback failed
    RollbackFailed(String),
    /// 超时错误
    /// Timeout error
    Timeout(String),
    /// 未知错误
    /// Unknown error
    Unknown(String),
}

impl std::fmt::Display for FogResetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FogResetError::CacheResetFailed(msg) => write!(f, "Cache reset failed: {msg}"),
            FogResetError::ChunkStateResetFailed(msg) => {
                write!(f, "Chunk state reset failed: {msg}")
            }
            FogResetError::ImageResetFailed(msg) => write!(f, "Image reset failed: {msg}"),
            FogResetError::TextureResetFailed(msg) => write!(f, "Texture reset failed: {msg}"),
            FogResetError::EntityCleanupFailed(msg) => write!(f, "Entity cleanup failed: {msg}"),
            FogResetError::RenderWorldFailed(msg) => {
                write!(f, "Render world processing failed: {msg}")
            }
            FogResetError::RollbackFailed(msg) => write!(f, "Rollback failed: {msg}"),
            FogResetError::Timeout(msg) => write!(f, "Timeout: {msg}"),
            FogResetError::Unknown(msg) => write!(f, "Unknown error: {msg}"),
        }
    }
}

impl std::error::Error for FogResetError {}

/// 纹理大小计算结果
/// Texture size calculation result
#[derive(Debug, Clone)]
pub struct TextureSizeInfo {
    /// 总字节数
    /// Total bytes
    pub total_bytes: usize,
    /// 每行字节数
    /// Bytes per row
    pub bytes_per_row: usize,
    /// 对齐后的每行字节数
    /// Aligned bytes per row
    pub aligned_bytes_per_row: usize,
    /// 原始尺寸
    /// Original dimensions
    pub width: u32,
    pub height: u32,
    pub depth_or_layers: u32,
}

/// 安全的纹理大小计算工具
/// Safe texture size calculation utilities
pub struct TextureSizeCalculator;

impl TextureSizeCalculator {
    /// 计算2D纹理的大小（单通道）
    /// Calculate 2D texture size (single channel)
    pub fn calculate_2d_single_channel(
        width: u32,
        height: u32,
    ) -> Result<TextureSizeInfo, FogResetError> {
        let bytes_per_pixel = 1u64; // Single channel (R8Unorm)

        let bytes_per_row = (width as u64)
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture width too large: {width}")))?;

        let total_bytes = bytes_per_row.checked_mul(height as u64).ok_or_else(|| {
            FogResetError::Unknown(format!("Texture size too large: {width}x{height}"))
        })?;

        let total_bytes_usize = usize::try_from(total_bytes).map_err(|_| {
            FogResetError::Unknown(format!("Texture size exceeds usize: {total_bytes}"))
        })?;

        let bytes_per_row_usize = usize::try_from(bytes_per_row).map_err(|_| {
            FogResetError::Unknown(format!("Bytes per row exceeds usize: {bytes_per_row}"))
        })?;

        // 对齐计算需要RenderDevice，这里先返回未对齐的值
        // Alignment calculation requires RenderDevice, return unaligned value for now
        Ok(TextureSizeInfo {
            total_bytes: total_bytes_usize,
            bytes_per_row: bytes_per_row_usize,
            aligned_bytes_per_row: bytes_per_row_usize, // Will be updated when alignment is available
            width,
            height,
            depth_or_layers: 1,
        })
    }

    /// 计算2D纹理的大小（RGBA）
    /// Calculate 2D texture size (RGBA)
    pub fn calculate_2d_rgba(width: u32, height: u32) -> Result<TextureSizeInfo, FogResetError> {
        let bytes_per_pixel = 4u64; // RGBA

        let bytes_per_row = (width as u64)
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture width too large: {width}")))?;

        let total_bytes = bytes_per_row.checked_mul(height as u64).ok_or_else(|| {
            FogResetError::Unknown(format!("Texture size too large: {width}x{height}"))
        })?;

        let total_bytes_usize = usize::try_from(total_bytes).map_err(|_| {
            FogResetError::Unknown(format!("Texture size exceeds usize: {total_bytes}"))
        })?;

        let bytes_per_row_usize = usize::try_from(bytes_per_row).map_err(|_| {
            FogResetError::Unknown(format!("Bytes per row exceeds usize: {bytes_per_row}"))
        })?;

        Ok(TextureSizeInfo {
            total_bytes: total_bytes_usize,
            bytes_per_row: bytes_per_row_usize,
            aligned_bytes_per_row: bytes_per_row_usize,
            width,
            height,
            depth_or_layers: 1,
        })
    }

    /// 计算3D纹理数组的大小（单通道）
    /// Calculate 3D texture array size (single channel)
    pub fn calculate_3d_single_channel(
        width: u32,
        height: u32,
        depth_or_layers: u32,
    ) -> Result<TextureSizeInfo, FogResetError> {
        let bytes_per_pixel = 1u64; // Single channel (R8Unorm)

        let bytes_per_row = (width as u64)
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture width too large: {width}")))?;

        let bytes_per_slice = bytes_per_row.checked_mul(height as u64).ok_or_else(|| {
            FogResetError::Unknown(format!("Texture slice too large: {width}x{height}"))
        })?;

        let total_bytes = bytes_per_slice
            .checked_mul(depth_or_layers as u64)
            .ok_or_else(|| {
                FogResetError::Unknown(format!(
                    "Texture array too large: {width}x{height}x{depth_or_layers}"
                ))
            })?;

        let total_bytes_usize = usize::try_from(total_bytes).map_err(|_| {
            FogResetError::Unknown(format!("Texture size exceeds usize: {total_bytes}"))
        })?;

        let bytes_per_row_usize = usize::try_from(bytes_per_row).map_err(|_| {
            FogResetError::Unknown(format!("Bytes per row exceeds usize: {bytes_per_row}"))
        })?;

        Ok(TextureSizeInfo {
            total_bytes: total_bytes_usize,
            bytes_per_row: bytes_per_row_usize,
            aligned_bytes_per_row: bytes_per_row_usize,
            width,
            height,
            depth_or_layers,
        })
    }

    /// 计算3D纹理数组的大小（RGBA）
    /// Calculate 3D texture array size (RGBA)
    pub fn calculate_3d_rgba(
        width: u32,
        height: u32,
        depth_or_layers: u32,
    ) -> Result<TextureSizeInfo, FogResetError> {
        let bytes_per_pixel = 4u64; // RGBA

        let bytes_per_row = (width as u64)
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture width too large: {width}")))?;

        let bytes_per_slice = bytes_per_row.checked_mul(height as u64).ok_or_else(|| {
            FogResetError::Unknown(format!("Texture slice too large: {width}x{height}"))
        })?;

        let total_bytes = bytes_per_slice
            .checked_mul(depth_or_layers as u64)
            .ok_or_else(|| {
                FogResetError::Unknown(format!(
                    "Texture array too large: {width}x{height}x{depth_or_layers}"
                ))
            })?;

        let total_bytes_usize = usize::try_from(total_bytes).map_err(|_| {
            FogResetError::Unknown(format!("Texture size exceeds usize: {total_bytes}"))
        })?;

        let bytes_per_row_usize = usize::try_from(bytes_per_row).map_err(|_| {
            FogResetError::Unknown(format!("Bytes per row exceeds usize: {bytes_per_row}"))
        })?;

        Ok(TextureSizeInfo {
            total_bytes: total_bytes_usize,
            bytes_per_row: bytes_per_row_usize,
            aligned_bytes_per_row: bytes_per_row_usize,
            width,
            height,
            depth_or_layers,
        })
    }
}

/// 事件：重置所有雾效数据，包括已探索区域、可见性状态和纹理数据。
/// Event: Reset all fog of war data, including explored areas, visibility states, and texture data.
#[derive(Event, Debug, Default)]
pub struct ResetFogOfWar;

/// 事件：雾效重置成功完成
/// Event: Fog of war reset completed successfully
#[derive(Event, Debug, Default)]
pub struct FogResetSuccess {
    /// 重置持续时间（毫秒）
    /// Reset duration in milliseconds
    pub duration_ms: u64,
    /// 重置的区块数量
    /// Number of chunks that were reset
    pub chunks_reset: usize,
}

/// 事件：雾效重置失败
/// Event: Fog of war reset failed
#[derive(Event, Debug)]
pub struct FogResetFailed {
    /// 失败原因
    /// Failure reason
    pub error: FogResetError,
    /// 重置持续时间（毫秒）
    /// Reset duration in milliseconds
    pub duration_ms: u64,
}

/// 重置同步状态
/// Reset synchronization state
#[derive(Debug, Clone, PartialEq)]
pub enum ResetSyncState {
    /// 空闲状态，无重置进行中
    /// Idle state, no reset in progress
    Idle,
    /// 主世界已发起重置，等待渲染世界处理
    /// Main world has initiated reset, waiting for render world to process
    MainWorldComplete,
    /// 渲染世界正在处理重置
    /// Render world is processing reset
    RenderWorldProcessing,
    /// 重置完成，等待清理
    /// Reset complete, waiting for cleanup
    Complete,
    /// 重置失败，需要回滚
    /// Reset failed, needs rollback
    Failed(FogResetError),
}

/// 资源：原子性的跨世界同步重置管理
/// Resource: Atomic cross-world synchronization reset management
#[derive(Resource, Debug, Clone, ExtractResource)]
pub struct FogResetSync {
    /// 当前同步状态
    /// Current synchronization state
    pub state: ResetSyncState,
    /// 重置开始时间戳（毫秒）
    /// Reset start timestamp (milliseconds)
    pub start_time: Option<u64>,
    /// 重置超时时间（毫秒）
    /// Reset timeout duration (milliseconds)
    pub timeout_ms: u64,
    /// 重置前的检查点数据
    /// Checkpoint data before reset
    pub checkpoint: Option<ResetCheckpoint>,
    /// 重置时的区块数量（用于统计）
    /// Number of chunks during reset (for statistics)
    pub chunks_count: usize,
}

/// 重置检查点，用于回滚
/// Reset checkpoint for rollback
#[derive(Debug, Clone)]
pub struct ResetCheckpoint {
    /// 探索区块集合的备份
    /// Backup of explored chunks set
    pub explored_chunks: std::collections::HashSet<bevy::math::IVec2>,
    /// 可见区块集合的备份
    /// Backup of visible chunks set
    pub visible_chunks: std::collections::HashSet<bevy::math::IVec2>,
    /// GPU驻留区块集合的备份
    /// Backup of GPU resident chunks set
    pub gpu_resident_chunks: std::collections::HashSet<bevy::math::IVec2>,
    /// 相机视图区块集合的备份
    /// Backup of camera view chunks set
    pub camera_view_chunks: std::collections::HashSet<bevy::math::IVec2>,
    /// 检查点创建时间
    /// Checkpoint creation time
    pub created_at: u64,
}

impl Default for FogResetSync {
    fn default() -> Self {
        Self {
            state: ResetSyncState::Idle,
            start_time: None,
            timeout_ms: 15000, // 15秒超时 / 15 second timeout
            checkpoint: None,
            chunks_count: 0,
        }
    }
}

impl FogResetSync {
    /// 检查重置是否超时
    /// Check if reset has timed out
    pub fn is_timeout(&self, current_time: u64) -> bool {
        if let Some(start_time) = self.start_time {
            current_time - start_time > self.timeout_ms
        } else {
            false
        }
    }

    /// 开始重置流程
    /// Start reset process
    pub fn start_reset(&mut self, current_time: u64) {
        self.state = ResetSyncState::MainWorldComplete;
        self.start_time = Some(current_time);
    }

    /// 标记渲染世界开始处理
    /// Mark render world processing started
    pub fn start_render_processing(&mut self) {
        if self.state == ResetSyncState::MainWorldComplete {
            self.state = ResetSyncState::RenderWorldProcessing;
        }
    }

    /// 标记重置完成
    /// Mark reset complete
    pub fn mark_complete(&mut self) {
        if self.state == ResetSyncState::RenderWorldProcessing {
            self.state = ResetSyncState::Complete;
        }
    }

    /// 标记重置失败
    /// Mark reset failed
    pub fn mark_failed(&mut self, error: FogResetError) {
        self.state = ResetSyncState::Failed(error);
    }

    /// 标记重置失败（字符串消息，转换为Unknown错误）
    /// Mark reset failed (string message, converted to Unknown error)
    pub fn mark_failed_str(&mut self, error: String) {
        self.state = ResetSyncState::Failed(FogResetError::Unknown(error));
    }

    /// 重置到空闲状态
    /// Reset to idle state
    pub fn reset_to_idle(&mut self) {
        self.state = ResetSyncState::Idle;
        self.start_time = None;
        self.checkpoint = None;
        self.chunks_count = 0;
    }

    /// 检查是否有可用的检查点进行回滚
    /// Check if checkpoint is available for rollback
    pub fn has_checkpoint(&self) -> bool {
        self.checkpoint.is_some()
    }

    /// 获取检查点的引用
    /// Get checkpoint reference
    pub fn get_checkpoint(&self) -> Option<&ResetCheckpoint> {
        self.checkpoint.as_ref()
    }
}
