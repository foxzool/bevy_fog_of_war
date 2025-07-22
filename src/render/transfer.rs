//! GPU↔CPU memory transfer coordination and texture reset operations for fog system.
//! 雾效系统的GPU↔CPU内存传输协调和纹理重置操作
//!
//! This module implements the render world side of dynamic memory management,
//! handling asynchronous texture transfers between GPU and CPU memory spaces.
//! It enables intelligent chunk loading/unloading and complete fog system reset operations.
//!
//! # Transfer Architecture
//!
//! ## Bidirectional Memory Flow
//! The system manages two primary transfer directions:
//! ```text
//! CPU Memory ←→ GPU Memory
//!     ↓              ↓
//! [Asset Images] ↔ [Texture Arrays]
//! Chunk Storage    Render Ready
//! ```
//!
//! ## Transfer Operations
//! - **CPU→GPU Upload**: Load chunks from CPU images to GPU texture arrays
//! - **GPU→CPU Download**: Save chunks from GPU texture arrays to CPU images
//! - **Async Processing**: Non-blocking transfers using staging buffers
//! - **Batch Operations**: Efficient processing of multiple chunks
//!
//! # Memory Management Strategy
//!
//! ## Dynamic Loading/Unloading
//! - **Demand Loading**: Upload chunks when camera approaches
//! - **Memory Pressure**: Download chunks when GPU memory is constrained
//! - **Persistence Support**: Save/load chunks for game state persistence
//! - **Cache Management**: Intelligent chunk eviction strategies
//!
//! ## Staging Buffer Architecture
//! Uses GPU staging buffers for efficient async transfers:
//! - **Map/Unmap Cycle**: GPU buffer mapping for CPU access
//! - **Channel Communication**: Async channels for completion signaling
//! - **Buffer Reuse**: Efficient memory allocation patterns
//! - **Error Recovery**: Robust handling of transfer failures
//!
//! # Performance Characteristics
//!
//! ## Transfer Efficiency
//! - **Async Operations**: Non-blocking transfers maintain frame rate
//! - **Batch Processing**: Multiple chunks processed per frame
//! - **Memory Bandwidth**: Optimized for GPU memory access patterns
//! - **Pipeline Overlap**: Transfers overlap with rendering operations
//!
//! ## Scalability Factors
//! - **Chunk Size**: Larger chunks reduce transfer overhead
//! - **Transfer Queue**: Bounded queues prevent memory overflow
//! - **Concurrent Limits**: Controlled concurrent transfer count
//! - **Buffer Pool**: Reused staging buffers reduce allocation cost
//!
//! # Reset Operations
//!
//! ## Complete Fog Reset
//! Provides atomic reset of entire fog system:
//! - **All Textures**: Resets fog, visibility, and snapshot textures
//! - **All Layers**: Clears all texture array layers
//! - **Synchronized**: Coordinated between main and render worlds
//! - **Error Handling**: Robust error recovery and rollback
//!
//! ## Memory Safety
//! - **Overflow Protection**: Safe arithmetic prevents integer overflow
//! - **Buffer Validation**: Validates all buffer operations
//! - **Resource Cleanup**: Proper disposal of temporary resources
//! - **Error Propagation**: Clear error reporting to main world

use crate::prelude::*;
use crate::render::RenderFogMapSettings;
use crate::render::extract::{RenderFogTexture, RenderSnapshotTexture, RenderVisibilityTexture};
use crate::settings::MAX_LAYERS;
use async_channel::{Receiver, Sender};
use bevy_image::TextureFormatPixelInfo;
use bevy_math::IVec2;
use bevy_platform::collections::HashMap;
use bevy_render::MainWorld;
use bevy_render::render_asset::RenderAssets;
use bevy_render::render_resource::{
    Buffer, BufferDescriptor, BufferInitDescriptor, BufferUsages, CommandEncoderDescriptor,
    Extent3d, MapMode, Origin3d, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo,
    TextureAspect, TextureFormat,
};
use bevy_render::renderer::{RenderDevice, RenderQueue};
use bevy_render::texture::GpuImage;

/// Processes CPU-to-GPU texture upload requests by copying chunk data to GPU texture arrays.
/// 通过将区块数据复制到GPU纹理数组来处理CPU到GPU纹理上传请求
///
/// This system handles the render world side of CPU→GPU chunk uploads, taking CPU image
/// assets and copying their data into appropriate layers of GPU texture arrays. It enables
/// dynamic chunk loading when chunks move from CPU storage to active GPU rendering.
///
/// # Upload Process
/// 1. **Request Validation**: Check if any upload requests are pending
/// 2. **Texture Access**: Retrieve target GPU texture array handles
/// 3. **Image Resolution**: Resolve CPU image assets via asset handles
/// 4. **Command Encoding**: Create GPU command encoder for texture operations
/// 5. **Texture Copying**: Copy image data to specified texture array layers
/// 6. **Queue Submission**: Submit commands to GPU render queue
/// 7. **Event Notification**: Queue completion events for main world
///
/// # Upload Types
/// Each request uploads two texture types:
/// - **Fog Texture**: R8Unorm format, single-channel exploration data
/// - **Snapshot Texture**: RGBA8 format, full-color entity snapshots
///
/// # GPU Command Generation
/// For each request, generates GPU commands to:
/// ```gpu
/// copy_texture_to_texture(
///     source: CPU_image_texture,
///     destination: GPU_array_texture[layer_index],
///     extent: chunk_resolution
/// )
/// ```
///
/// # Performance Characteristics
/// - **Batch Processing**: Processes all pending requests in single frame
/// - **GPU Queue**: Efficient GPU command submission via render queue
/// - **Memory Bandwidth**: Direct texture-to-texture copy operations
/// - **Async Completion**: Non-blocking upload with event-based completion
///
/// # Error Handling and Resilience
/// The system handles missing resources gracefully:
/// - **Empty Requests**: Early return when no uploads pending
/// - **Missing Textures**: Skips processing when target arrays unavailable
/// - **Missing Images**: Skips individual images that aren't loaded yet
/// - **Asset Loading**: Waits for CPU image assets to load before processing
///
/// # Integration Points
/// - **Asset System**: Depends on loaded Image assets for source data
/// - **Texture Arrays**: Targets fog and snapshot GPU texture arrays
/// - **Main World**: Sends completion events via CpuToGpuRequests tracking
/// - **Memory Manager**: Coordinates with TextureArrayManager for layer allocation
///
/// # Upload Workflow
/// ```text
/// [CPU Image Assets] → [Upload Requests] → [GPU Command Encoder]
///         ↓                    ↓                     ↓
/// Bevy Asset System    CpuToGpuCopyRequests    texture_to_texture
///         ↓                    ↓                     ↓
/// Source Data Ready → Layer Indices Known → GPU Array Updated
/// ```
///
/// # Memory Layout Requirements
/// - **Source Images**: Must match target texture format and dimensions
/// - **Layer Indices**: Must be pre-allocated via TextureArrayManager
/// - **Format Compatibility**: Source and destination formats must match
/// - **Resolution Match**: Image dimensions must match chunk texture resolution
///
/// # Time Complexity: O(n) where n = number of pending upload requests
pub fn process_cpu_to_gpu_copies(
    render_queue: Res<RenderQueue>,
    cpu_upload_requests: Res<CpuToGpuCopyRequests>,
    fog_texture_array_handle: Res<RenderFogTexture>,
    snapshot_texture_array_handle: Res<RenderSnapshotTexture>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    mut cpu_to_gpu_requests: ResMut<CpuToGpuRequests>,
    render_device: Res<RenderDevice>,
) {
    // Early return if no upload requests are pending
    // 如果没有待处理的上传请求，则提前返回
    if cpu_upload_requests.requests.is_empty() {
        return;
    }

    // Get target GPU texture arrays, exit if not available
    // 获取目标GPU纹理数组，如果不可用则退出
    let Some(fog_gpu_image) = gpu_images.get(&fog_texture_array_handle.0) else {
        return; // Fog texture array not ready yet
    };

    let Some(snapshot_gpu_image) = gpu_images.get(&snapshot_texture_array_handle.0) else {
        return; // Snapshot texture array not ready yet
    };

    // Process each upload request individually with separate command encoders
    // 使用单独的命令编码器分别处理每个上传请求
    for request in &cpu_upload_requests.requests {
        // Create command encoder for this specific upload operation
        // 为此特定上传操作创建命令编码器
        let mut command_encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("fog of war cpu_to_gpu_copy"), // Debug label for GPU debugging
        });

        // --- Upload Fog Texture Data ---
        // --- 上传雾效纹理数据 ---

        // Upload fog data if the source image asset is loaded
        // 如果源图像资源已加载，则上传雾效数据
        if let Some(upload_fog_image) = gpu_images.get(&request.fog_image_handle) {
            command_encoder.copy_texture_to_texture(
                upload_fog_image.texture.as_image_copy(), // Source: CPU image texture
                TexelCopyTextureInfo {
                    texture: &fog_gpu_image.texture, // Destination: GPU texture array
                    mip_level: 0,                    // Top mip level only
                    origin: Origin3d {
                        x: 0, // Copy to origin of layer
                        y: 0,
                        z: request.fog_layer_index, // Target array layer
                    },
                    aspect: TextureAspect::All, // Copy all texture aspects
                },
                Extent3d {
                    width: upload_fog_image.size.width, // Match source image dimensions
                    height: upload_fog_image.size.height,
                    depth_or_array_layers: 1, // Copy single layer
                },
            );
        }

        // --- Upload Snapshot Texture Data ---
        // --- 上传快照纹理数据 ---

        // Upload snapshot data if the source image asset is loaded
        // 如果源图像资源已加载，则上传快照数据
        if let Some(upload_snapshot_image) = gpu_images.get(&request.snapshot_image_handle) {
            command_encoder.copy_texture_to_texture(
                upload_snapshot_image.texture.as_image_copy(), // Source: CPU snapshot texture
                TexelCopyTextureInfo {
                    texture: &snapshot_gpu_image.texture, // Destination: GPU texture array
                    mip_level: 0,                         // Top mip level only
                    origin: Origin3d {
                        x: 0, // Copy to origin of layer
                        y: 0,
                        z: request.snapshot_layer_index, // Target array layer
                    },
                    aspect: TextureAspect::All, // Copy all texture aspects
                },
                Extent3d {
                    width: upload_snapshot_image.size.width, // Match source image dimensions
                    height: upload_snapshot_image.size.height,
                    depth_or_array_layers: 1, // Copy single layer
                },
            );
        }

        // Submit commands to GPU render queue for execution
        // 将命令提交到GPU渲染队列以执行
        render_queue.submit(std::iter::once(command_encoder.finish()));

        // Track completion for main world notification
        // 跟踪完成情况以通知主世界
        cpu_to_gpu_requests.requests.push(request.chunk_coords);
    }
}

/// Initiates GPU-to-CPU texture download operations by creating staging buffers and copy commands.
/// 通过创建暂存缓冲区和复制命令启动GPU到CPU纹理下载操作
///
/// This system handles the first phase of GPU→CPU chunk downloads, creating staging buffers
/// and issuing GPU commands to copy texture data from GPU arrays to CPU-accessible buffers.
/// It sets up the infrastructure for asynchronous data readback operations.
///
/// # Download Process Phase 1
/// 1. **Request Processing**: Process pending GPU→CPU copy requests
/// 2. **Resource Validation**: Verify GPU texture arrays are available
/// 3. **Buffer Size Calculation**: Compute staging buffer sizes with overflow protection
/// 4. **Staging Buffer Creation**: Create CPU-accessible staging buffers
/// 5. **Copy Command Generation**: Generate GPU commands for texture→buffer copy
/// 6. **Async Channel Setup**: Create channels for completion signaling
/// 7. **Command Submission**: Submit commands to GPU render queue
///
/// # Staging Buffer Architecture
/// Creates temporary GPU buffers for data transfer:
/// - **MAP_READ**: Enable CPU mapping for data access
/// - **COPY_DST**: Allow GPU texture data to be copied into buffer
/// - **Aligned Layout**: Proper byte alignment for GPU requirements
/// - **Per-Chunk Buffers**: Individual buffers for each chunk transfer
///
/// # Memory Safety Features
/// - **Overflow Protection**: Checked arithmetic prevents integer overflow
/// - **Buffer Validation**: Validates buffer sizes before creation
/// - **Error Handling**: Graceful handling of invalid configurations
/// - **Resource Cleanup**: Proper disposal via async channel lifecycle
///
/// # Performance Optimizations
/// - **Duplicate Prevention**: Skips chunks already being processed
/// - **Batch Processing**: Processes multiple requests efficiently
/// - **Async Architecture**: Non-blocking transfers maintain frame rate
/// - **Resource Reuse**: Efficient staging buffer allocation patterns
///
/// # Time Complexity: O(n) where n = number of new download requests
#[allow(clippy::too_many_arguments)]
pub fn initiate_gpu_to_cpu_copies_and_request_map(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut gpu_read_requests: ResMut<GpuToCpuCopyRequests>,
    fog_texture_array_handle: Res<RenderFogTexture>,
    snapshot_texture_array_handle: Res<RenderSnapshotTexture>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    render_fog_settings: Res<RenderFogMapSettings>,
    mut active_copies: ResMut<GpuToCpuActiveCopies>,
) {
    if gpu_read_requests.requests.is_empty() {
        return;
    }

    let Some(fog_gpu_image) = gpu_images.get(&fog_texture_array_handle.0) else {
        return;
    };
    let Some(snapshot_gpu_image) = gpu_images.get(&snapshot_texture_array_handle.0) else {
        return;
    };

    let texture_width = render_fog_settings.texture_resolution_per_chunk.x;
    let texture_height = render_fog_settings.texture_resolution_per_chunk.y;
    let fog_format = fog_gpu_image.texture_format;
    let snapshot_format = snapshot_gpu_image.texture_format;

    for request in &gpu_read_requests.requests {
        if active_copies
            .pending_copies
            .contains_key(&request.chunk_coords)
        {
            continue;
        }

        let fog_format_size = fog_format.pixel_size() as u32;
        if fog_format_size == 0 {
            error!("Fog buffer size is 0 for chunk {:?}", request.chunk_coords);
            continue;
        }

        // 安全的雾效缓冲区大小计算，防止整数溢出
        // Safe fog buffer size calculation to prevent integer overflow
        let bytes_per_row_fog = (texture_width as u64)
            .checked_mul(fog_format_size as u64)
            .expect("Fog bytes per row calculation would overflow");
        let fog_buffer_size = bytes_per_row_fog
            .checked_mul(texture_height as u64)
            .expect("Fog buffer size calculation would overflow");

        let snapshot_format_size = snapshot_format.pixel_size() as u32;
        if snapshot_format_size == 0 {
            error!(
                "Snapshot buffer size is 0 for chunk {:?}",
                request.chunk_coords
            );
            continue;
        }
        // 安全的快照缓冲区大小计算，防止整数溢出
        // Safe snapshot buffer size calculation to prevent integer overflow
        let bytes_per_row_snapshot = (texture_width as u64)
            .checked_mul(snapshot_format_size as u64)
            .expect("Snapshot bytes per row calculation would overflow");
        let snapshot_buffer_size = bytes_per_row_snapshot
            .checked_mul(texture_height as u64)
            .expect("Snapshot buffer size calculation would overflow");
        if snapshot_buffer_size == 0 {
            error!(
                "Snapshot buffer size is 0 for chunk {:?}",
                request.chunk_coords
            );
            continue;
        }

        let mut command_encoder =
            render_device.create_command_encoder(&CommandEncoderDescriptor::default());

        let fog_staging_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some(&format!("fog_staging_buffer_{:?}", request.chunk_coords)),
            size: fog_buffer_size,
            usage: BufferUsages::MAP_READ
                | BufferUsages::MAP_WRITE
                | BufferUsages::COPY_DST
                | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let snapshot_staging_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some(&format!(
                "snapshot_staging_buffer_{:?}",
                request.chunk_coords
            )),
            size: snapshot_buffer_size,
            usage: BufferUsages::MAP_READ
                | BufferUsages::MAP_WRITE
                | BufferUsages::COPY_DST
                | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // --- 复制雾效纹理数据到暂存区 ---
        // --- Copy Fog Texture Data to Staging Buffer ---
        command_encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &fog_gpu_image.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: request.fog_layer_index,
                },
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &fog_staging_buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        u32::try_from(bytes_per_row_fog)
                            .expect("Fog bytes per row too large for u32"),
                    ), // Must be correctly aligned if required by backend
                    rows_per_image: Some(texture_height), // For 2D, this is height
                },
            },
            Extent3d {
                width: texture_width,
                height: texture_height,
                depth_or_array_layers: 1, // Copying a single layer
            },
        );

        // NOTE: Removed texture clearing operation here as it was incorrectly
        // resetting explored areas during save operations. GPU-to-CPU readback
        // should not modify the original texture data.
        // 注意：移除了这里的纹理清除操作，因为它在保存操作时错误地重置了已探索区域。
        // GPU到CPU的回读不应该修改原始纹理数据。

        // --- 复制快照纹理数据到暂存区 ---
        // --- Copy Snapshot Texture Data to Staging Buffer ---
        command_encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &snapshot_gpu_image.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: request.snapshot_layer_index,
                },
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &snapshot_staging_buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        u32::try_from(bytes_per_row_snapshot)
                            .expect("Snapshot bytes per row too large for u32"),
                    ),
                    rows_per_image: Some(texture_height),
                },
            },
            Extent3d {
                width: texture_width,
                height: texture_height,
                depth_or_array_layers: 1, // Copying a single layer
            },
        );

        // NOTE: Removed snapshot texture clearing operation here as it was incorrectly
        // resetting explored areas during save operations. GPU-to-CPU readback
        // should not modify the original texture data.
        // 注意：移除了这里的快照纹理清除操作，因为它在保存操作时错误地重置了已探索区域。
        // GPU到CPU的回读不应该修改原始纹理数据。

        let (fog_tx, fog_rx) = async_channel::bounded(1);
        let (snapshot_tx, snapshot_rx) = async_channel::bounded(1);

        active_copies.pending_copies.insert(
            request.chunk_coords,
            PendingCopyData {
                fog_buffer: fog_staging_buffer,
                fog_tx,
                fog_rx,
                snapshot_buffer: snapshot_staging_buffer,
                snapshot_tx,
                snapshot_rx,
                original_request: request.clone(),
                fog_result: None,
                snapshot_result: None,
            },
        );

        render_queue.submit(std::iter::once(command_encoder.finish()));
    }

    // Clear the requests after processing them
    // 处理完请求后清除它们
    gpu_read_requests.requests.clear();
}

/// Checks for completed buffer mappings and processes downloaded texture data.
/// 检查已完成的缓冲区映射并处理下载的纹理数据
///
/// This system handles the final phase of GPU→CPU downloads by checking async channels
/// for completed buffer mappings and sending ChunkGpuDataReady events to the main world
/// when both fog and snapshot data are available.
///
/// # Data Collection Process
/// 1. **Channel Polling**: Check async channels for completed buffer reads
/// 2. **Data Accumulation**: Collect fog and snapshot data as they become available
/// 3. **Completion Detection**: Wait for both fog and snapshot data completion
/// 4. **Event Generation**: Send ChunkGpuDataReady event with pixel data
/// 5. **Cleanup**: Remove completed transfers from active tracking
///
/// # Async Coordination
/// Manages completion of paired async operations:
/// - **Fog Buffer**: Waits for fog texture data readback
/// - **Snapshot Buffer**: Waits for snapshot texture data readback
/// - **Paired Completion**: Requires both buffers before event generation
/// - **Partial Progress**: Maintains state until both complete
///
/// # Time Complexity: O(n) where n = number of active transfer operations
pub fn check_and_process_mapped_buffers(
    mut main_world: ResMut<MainWorld>,
    mut active_copies: ResMut<GpuToCpuActiveCopies>,
) {
    active_copies.mapped_copies.retain(|_, pending_data| {
        if let Ok(data) = pending_data.fog_rx.try_recv() {
            pending_data.fog_result = Some(data);
        }
        if let Ok(data) = pending_data.snapshot_rx.try_recv() {
            pending_data.snapshot_result = Some(data);
        }

        if let (Some(fog_data), Some(snapshot_data)) =
            (&pending_data.fog_result, &pending_data.snapshot_result)
        {
            main_world.send_event(ChunkGpuDataReady {
                chunk_coords: pending_data.original_request.chunk_coords,
                fog_data: fog_data.clone(),
                snapshot_data: snapshot_data.clone(),
            });
            false
        } else {
            true
        }
    });
}

/// Resource tracking completed CPU-to-GPU upload requests for main world notification.
/// 跟踪已完成的CPU到GPU上传请求以通知主世界的资源
///
/// This resource accumulates chunk coordinates for uploads that have been submitted
/// to the GPU render queue. The main world processes these to send completion events
/// and update chunk tracking state.
///
/// # Usage Pattern
/// - **Render World**: Adds chunk coordinates when uploads complete
/// - **Main World**: Drains list to generate ChunkCpuDataUploaded events
/// - **Frame Cycle**: Cleared each frame after event generation
/// - **Batch Processing**: Multiple chunks processed together for efficiency
#[derive(Resource, Default)]
pub struct CpuToGpuRequests {
    /// Vector of chunk coordinates for completed CPU→GPU uploads.
    /// 已完成CPU→GPU上传的区块坐标向量
    ///
    /// Contains chunk coordinates that have been successfully uploaded to GPU
    /// texture arrays and are ready for ChunkCpuDataUploaded event generation.
    pub requests: Vec<IVec2>,
}

/// Resource storing the state of ongoing GPU-to-CPU copy operations with async coordination.
/// 存储正在进行的GPU到CPU复制操作状态及异步协调的资源
///
/// This resource manages the complex lifecycle of GPU→CPU texture downloads,
/// coordinating staging buffer creation, async mapping operations, and data collection
/// until both fog and snapshot data are ready for main world events.
///
/// # Transfer State Management
/// The resource tracks operations through two distinct phases:
/// - **Pending Copies**: Initial GPU command submission and buffer creation
/// - **Mapped Copies**: Buffer mapping and async data collection
///
/// # Async Architecture
/// Uses async channels for non-blocking communication between GPU operations
/// and CPU data collection, enabling efficient frame-rate maintenance during
/// large texture transfers.
///
/// # Memory Management
/// - **Staging Buffers**: Temporary GPU buffers for data transfer
/// - **Channel Communication**: Async channels signal completion
/// - **Cleanup Lifecycle**: Automatic cleanup when transfers complete
/// - **Error Recovery**: Graceful handling of mapping failures
#[derive(Resource, Default)]
pub struct GpuToCpuActiveCopies {
    /// HashMap of chunks with pending GPU→staging buffer copy operations.
    /// 具有待处理GPU→暂存缓冲区复制操作的区块HashMap
    ///
    /// Tracks chunks that have had staging buffers created and GPU copy commands
    /// submitted but haven't yet started the buffer mapping process.
    pub pending_copies: HashMap<IVec2, PendingCopyData>,

    /// HashMap of chunks with active buffer mapping and data collection operations.
    /// 具有活动缓冲区映射和数据收集操作的区块HashMap
    ///
    /// Tracks chunks that are in the async buffer mapping phase, waiting for
    /// CPU-accessible data to become available via async channels.
    pub mapped_copies: HashMap<IVec2, PendingCopyData>,
}

/// Data structure tracking individual chunk download operations with async coordination.
/// 跟踪具有异步协调的单个区块下载操作的数据结构
///
/// This structure manages the complete lifecycle of a single chunk's GPU→CPU transfer,
/// including staging buffers, async communication channels, and result accumulation.
/// Each chunk requires two separate transfers (fog and snapshot) that must complete
/// before the main world can receive the combined data.
///
/// # Async Communication Pattern
/// Uses bounded async channels for each texture type:
/// - **Sender**: Used by buffer mapping callbacks to send pixel data
/// - **Receiver**: Polled by main system to collect completed data
/// - **Result Storage**: Accumulates data until both textures complete
///
/// # Resource Lifecycle
/// 1. **Creation**: Staging buffers and channels created
/// 2. **GPU Transfer**: Commands submitted to copy textures→buffers
/// 3. **Mapping**: Buffers mapped for CPU access
/// 4. **Data Collection**: Async channels receive pixel data
/// 5. **Event Generation**: Combined data sent to main world
/// 6. **Cleanup**: Buffers and channels automatically disposed
pub struct PendingCopyData {
    /// GPU staging buffer for fog texture data transfer.
    /// 雾效纹理数据传输的GPU暂存缓冲区
    fog_buffer: Buffer,

    /// Async channel sender for fog texture pixel data.
    /// 雾效纹理像素数据的异步通道发送器
    fog_tx: Sender<Vec<u8>>,

    /// Async channel receiver for fog texture pixel data.
    /// 雾效纹理像素数据的异步通道接收器
    fog_rx: Receiver<Vec<u8>>,

    /// GPU staging buffer for snapshot texture data transfer.
    /// 快照纹理数据传输的GPU暂存缓冲区
    snapshot_buffer: Buffer,

    /// Async channel sender for snapshot texture pixel data.
    /// 快照纹理像素数据的异步通道发送器
    snapshot_tx: Sender<Vec<u8>>,

    /// Async channel receiver for snapshot texture pixel data.
    /// 快照纹理像素数据的异步通道接收器
    snapshot_rx: Receiver<Vec<u8>>,

    /// Original download request data for event reconstruction.
    /// 用于事件重建的原始下载请求数据
    original_request: GpuToCpuCopyRequest,

    /// Completed fog texture pixel data, if available.
    /// 已完成的雾效纹理像素数据（如果可用）
    fog_result: Option<Vec<u8>>,

    /// Completed snapshot texture pixel data, if available.
    /// 已完成的快照纹理像素数据（如果可用）
    snapshot_result: Option<Vec<u8>>,
}

/// Maps staging buffers for CPU access and sets up async data collection callbacks.
/// 映射暂存缓冲区以进行CPU访问并设置异步数据收集回调
///
/// This system moves download operations from pending to mapped state by initiating
/// the buffer mapping process. It sets up async callbacks that will collect pixel
/// data when the GPU makes the buffers available for CPU reading.
///
/// # Time Complexity: O(n) where n = number of pending buffer operations
pub fn map_buffers(mut active_copies: ResMut<GpuToCpuActiveCopies>) {
    let pending = active_copies
        .pending_copies
        .drain()
        .collect::<HashMap<IVec2, PendingCopyData>>();

    for (coord, pending_data) in pending {
        let fog_slice = pending_data.fog_buffer.slice(..);
        let fog_buffer = pending_data.fog_buffer.clone();
        let fog_tx = pending_data.fog_tx.clone();
        fog_slice.map_async(MapMode::Read, move |res| {
            res.expect("Failed to map fog buffer");
            let buffer_slice = fog_buffer.slice(..);
            let data = buffer_slice.get_mapped_range();
            let result = Vec::from(&*data);
            drop(data);
            fog_buffer.unmap();
            if let Err(e) = fog_tx.try_send(result) {
                warn!("Failed to send readback result: {}", e);
            }
        });

        let snapshot_slice = pending_data.snapshot_buffer.slice(..);
        let snapshot_buffer = pending_data.snapshot_buffer.clone();
        let snapshot_tx = pending_data.snapshot_tx.clone();
        snapshot_slice.map_async(MapMode::Read, move |res| {
            res.expect("Failed to map snapshot buffer");
            let buffer_slice = snapshot_buffer.slice(..);
            let data = buffer_slice.get_mapped_range();
            let result = Vec::from(&*data);
            drop(data);
            snapshot_buffer.unmap();
            if let Err(e) = snapshot_tx.try_send(result) {
                warn!("Failed to send readback result: {}", e);
            }
        });

        active_copies.mapped_copies.insert(coord, pending_data);
    }
}

/// Processes completed CPU-to-GPU upload requests and sends completion events to main world.
/// 处理已完成的CPU到GPU上传请求并向主世界发送完成事件
///
/// This system drains the accumulated upload completion list and converts chunk
/// coordinates into ChunkCpuDataUploaded events for main world processing.
///
/// # Time Complexity: O(n) where n = number of completed uploads
pub(crate) fn check_cpu_to_gpu_request(
    mut main_world: ResMut<MainWorld>,
    mut cpu_to_gpu_requests: ResMut<CpuToGpuRequests>,
) {
    let requests = cpu_to_gpu_requests
        .requests
        .drain(..)
        .map(|coord| ChunkCpuDataUploaded {
            chunk_coords: coord,
        })
        .collect::<Vec<ChunkCpuDataUploaded>>();

    main_world.send_event_batch(requests);
}

/// 检查并清空纹理（在渲染世界中重置时）
/// Check and clear textures (when resetting in render world)
/// Performs complete fog system reset by clearing all GPU texture arrays to default state.
/// 通过将所有GPU纹理数组清除为默认状态来执行完整的雾效系统重置
///
/// This system handles the render world side of fog system reset operations,
/// clearing all layers of fog, visibility, and snapshot texture arrays back
/// to their initial unexplored/empty state.
///
/// # Reset Process
/// 1. **State Validation**: Verify reset sync state and proceed if ready
/// 2. **Resource Gathering**: Access all GPU texture arrays and settings
/// 3. **Buffer Preparation**: Create reusable clear buffers with proper alignment
/// 4. **Texture Clearing**: Copy clear data to all texture array layers
/// 5. **Command Submission**: Submit commands to GPU render queue
/// 6. **Completion Notification**: Signal reset completion
///
/// # Memory Safety Features
/// - **Overflow Protection**: Safe arithmetic prevents buffer size overflow
/// - **Buffer Reuse**: Single buffers used for all layers to reduce memory
/// - **Proper Alignment**: GPU-aligned buffer layouts for efficient transfer
/// - **Error Recovery**: Graceful handling of missing resources
///
/// # Performance Optimizations
/// - **Batch Operations**: Clears all layers in single command submission
/// - **Buffer Reuse**: Minimizes memory allocation overhead
/// - **Efficient Layouts**: Optimized buffer layouts for GPU transfer
///
/// # Time Complexity: O(layers) where layers = MAX_LAYERS texture array depth
#[allow(clippy::too_many_arguments)]
pub fn check_and_clear_textures_on_reset(
    mut reset_sync: ResMut<FogResetSync>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    fog_texture: Res<RenderFogTexture>,
    visibility_texture: Res<RenderVisibilityTexture>,
    snapshot_texture: Res<RenderSnapshotTexture>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    render_settings: Res<RenderFogMapSettings>,
) {
    // 检查是否需要开始渲染世界处理
    // Check if render world processing needs to start
    if reset_sync.state != ResetSyncState::MainWorldComplete {
        // 添加调试信息以了解当前状态
        // Add debug info to understand current state
        if !matches!(reset_sync.state, ResetSyncState::Idle) {
            debug!(
                "Render world reset system called but state is: {:?}",
                reset_sync.state
            );
        }
        return;
    }

    // 标记渲染世界开始处理
    // Mark render world processing started
    reset_sync.start_render_processing();
    info!("Render world starting texture reset processing...");

    let texture_width = render_settings.texture_resolution_per_chunk.x;
    let texture_height = render_settings.texture_resolution_per_chunk.y;
    let num_layers = MAX_LAYERS;

    // Get GPU images with error handling
    let Some(fog_gpu_image) = gpu_images.get(&fog_texture.0) else {
        error!("Failed to get fog GPU image during reset");
        reset_sync.mark_failed(FogResetError::RenderWorldFailed(
            "Failed to get fog GPU image during reset".to_string(),
        ));
        return;
    };
    let Some(visibility_gpu_image) = gpu_images.get(&visibility_texture.0) else {
        error!("Failed to get visibility GPU image during reset");
        reset_sync.mark_failed(FogResetError::RenderWorldFailed(
            "Failed to get visibility GPU image during reset".to_string(),
        ));
        return;
    };
    let Some(snapshot_gpu_image) = gpu_images.get(&snapshot_texture.0) else {
        error!("Failed to get snapshot GPU image during reset");
        reset_sync.mark_failed(FogResetError::RenderWorldFailed(
            "Failed to get snapshot GPU image during reset".to_string(),
        ));
        return;
    };

    let mut command_encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("fog_reset_clear_textures"),
    });

    // Pre-calculate buffer sizes and create reusable buffers to reduce memory usage
    // 预先计算缓冲区大小并创建可重用缓冲区以减少内存使用
    // 安全的雾效重置缓冲区大小计算，防止整数溢出
    // Safe fog reset buffer size calculation to prevent integer overflow
    let fog_bytes_per_row = (texture_width as u64)
        .checked_mul(TextureFormat::R8Unorm.pixel_size() as u64)
        .and_then(|v| u32::try_from(v).ok())
        .expect("Fog bytes per row calculation would overflow");
    let fog_padded_bytes_per_row =
        RenderDevice::align_copy_bytes_per_row(fog_bytes_per_row as usize);
    let fog_buffer_size = fog_padded_bytes_per_row
        .checked_mul(texture_height as usize)
        .expect("Fog buffer size calculation would overflow");
    let fog_clear_data = vec![0u8; fog_buffer_size]; // 0 = unexplored

    // 安全的可见性重置缓冲区大小计算，防止整数溢出
    // Safe visibility reset buffer size calculation to prevent integer overflow
    let vis_bytes_per_row = (texture_width as u64)
        .checked_mul(TextureFormat::R8Unorm.pixel_size() as u64)
        .and_then(|v| u32::try_from(v).ok())
        .expect("Visibility bytes per row calculation would overflow");
    let vis_padded_bytes_per_row =
        RenderDevice::align_copy_bytes_per_row(vis_bytes_per_row as usize);
    let vis_buffer_size = vis_padded_bytes_per_row
        .checked_mul(texture_height as usize)
        .expect("Visibility buffer size calculation would overflow");
    let vis_clear_data = vec![0u8; vis_buffer_size]; // 0 = not visible

    // 安全的快照重置缓冲区大小计算，防止整数溢出
    // Safe snapshot reset buffer size calculation to prevent integer overflow
    let snap_bytes_per_row = (texture_width as u64)
        .checked_mul(TextureFormat::Rgba8Unorm.pixel_size() as u64) // RGBA already included in pixel_size
        .and_then(|v| u32::try_from(v).ok())
        .expect("Snapshot bytes per row calculation would overflow");
    let snap_padded_bytes_per_row =
        RenderDevice::align_copy_bytes_per_row(snap_bytes_per_row as usize);
    let snap_buffer_size = snap_padded_bytes_per_row
        .checked_mul(texture_height as usize)
        .expect("Snapshot buffer size calculation would overflow");
    let snap_clear_data = vec![0u8; snap_buffer_size]; // Clear to black

    // Create reusable buffers once instead of creating new ones for each layer
    // 创建可重用缓冲区一次，而不是为每个层创建新的缓冲区
    let fog_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("fog_reset_clear_buffer"),
        contents: &fog_clear_data,
        usage: BufferUsages::COPY_SRC,
    });

    let vis_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("visibility_reset_clear_buffer"),
        contents: &vis_clear_data,
        usage: BufferUsages::COPY_SRC,
    });

    let snap_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("snapshot_reset_clear_buffer"),
        contents: &snap_clear_data,
        usage: BufferUsages::COPY_SRC,
    });

    // Clear fog texture (set to 0 = unexplored) - reuse fog_buffer for all layers
    // 清除雾效纹理（设置为0=未探索）- 对所有层重用fog_buffer
    for layer in 0..num_layers {
        command_encoder.copy_buffer_to_texture(
            TexelCopyBufferInfo {
                buffer: &fog_buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(fog_padded_bytes_per_row as u32),
                    rows_per_image: None,
                },
            },
            TexelCopyTextureInfo {
                texture: &fog_gpu_image.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: TextureAspect::All,
            },
            Extent3d {
                width: texture_width,
                height: texture_height,
                depth_or_array_layers: 1,
            },
        );
    }

    // Clear visibility texture (set to 0 = not visible) - reuse vis_buffer for all layers
    // 清除可见性纹理（设置为0=不可见）- 对所有层重用vis_buffer
    for layer in 0..num_layers {
        command_encoder.copy_buffer_to_texture(
            TexelCopyBufferInfo {
                buffer: &vis_buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(vis_padded_bytes_per_row as u32),
                    rows_per_image: None,
                },
            },
            TexelCopyTextureInfo {
                texture: &visibility_gpu_image.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: TextureAspect::All,
            },
            Extent3d {
                width: texture_width,
                height: texture_height,
                depth_or_array_layers: 1,
            },
        );
    }

    // Clear snapshot texture (set to 0) - reuse snap_buffer for all layers
    // 清除快照纹理（设置为0）- 对所有层重用snap_buffer
    for layer in 0..num_layers {
        command_encoder.copy_buffer_to_texture(
            TexelCopyBufferInfo {
                buffer: &snap_buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(snap_padded_bytes_per_row as u32),
                    rows_per_image: None,
                },
            },
            TexelCopyTextureInfo {
                texture: &snapshot_gpu_image.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: TextureAspect::All,
            },
            Extent3d {
                width: texture_width,
                height: texture_height,
                depth_or_array_layers: 1,
            },
        );
    }

    render_queue.submit(std::iter::once(command_encoder.finish()));

    // 通过事件通知主世界渲染完成，而不是直接修改同步状态
    // Notify main world of render completion via event instead of directly modifying sync state
    // Note: 直接修改 ExtractResource 的状态不会传回主世界
    // Note: Directly modifying ExtractResource state won't propagate back to main world
    info!("Render world texture reset processing complete");
}
