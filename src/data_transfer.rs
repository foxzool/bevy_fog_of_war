use crate::prelude::*;
use bevy::asset::Handle;
use bevy::ecs::event::Event;
use bevy::math::IVec2;
use bevy::prelude::Image;
use bevy::prelude::Resource;
use bevy::reflect::Reflect;
use bevy::render::extract_resource::ExtractResource;

/// 由主世界填充，请求渲染世界将 GPU 纹理数据复制到 CPU。
/// Populated by the main world to request the render world to copy GPU texture data to CPU.
#[derive(Resource, Default, Debug, Clone, Reflect, ExtractResource)]
#[reflect(Resource, Default)]
pub struct GpuToCpuCopyRequests {
    pub requests: Vec<GpuToCpuCopyRequest>,
}

#[derive(Debug, Clone, Reflect)]
pub struct GpuToCpuCopyRequest {
    pub chunk_coords: IVec2,
    pub fog_layer_index: u32,
    pub snapshot_layer_index: u32,
    // Staging buffer index or some identifier if RenderApp uses a pool
    // 如果 RenderApp 使用池，则为暂存缓冲区索引或某种标识符
}
/// 由主世界填充，请求渲染世界将 CPU 纹理数据上传到 GPU。
/// Populated by the main world to request the render world to upload CPU texture data to GPU.
#[derive(Resource, Default, Debug, Clone, Reflect, ExtractResource)]
#[reflect(Resource, Default)]
pub struct CpuToGpuCopyRequests {
    pub requests: Vec<CpuToGpuCopyRequest>,
}

#[derive(Debug, Clone, Reflect)]
pub struct CpuToGpuCopyRequest {
    pub chunk_coords: IVec2,
    pub fog_layer_index: u32,
    pub snapshot_layer_index: u32,
    pub fog_image_handle: Handle<Image>, // Handle to the Image asset in CPU memory
    pub snapshot_image_handle: Handle<Image>, // Handle to the Image asset in CPU memory
}

/// 事件：当 GPU 数据成功复制到 CPU 并可供主世界使用时，由 RenderApp 发送。
/// Event: Sent by RenderApp when GPU data has been successfully copied to CPU and is available to the main world.
#[derive(Event, Debug)]
pub struct ChunkGpuDataReadyEvent {
    pub chunk_coords: IVec2,
    pub fog_data: Vec<u8>,
    pub snapshot_data: Vec<u8>,
}

/// 事件：当 CPU 数据成功上传到 GPU 时，由 RenderApp 发送。
/// Event: Sent by RenderApp when CPU data has been successfully uploaded to GPU.
#[derive(Event, Debug)]
pub struct ChunkCpuDataUploadedEvent {
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
            FogResetError::ChunkStateResetFailed(msg) => write!(f, "Chunk state reset failed: {msg}"),
            FogResetError::ImageResetFailed(msg) => write!(f, "Image reset failed: {msg}"),
            FogResetError::TextureResetFailed(msg) => write!(f, "Texture reset failed: {msg}"),
            FogResetError::EntityCleanupFailed(msg) => write!(f, "Entity cleanup failed: {msg}"),
            FogResetError::RenderWorldFailed(msg) => write!(f, "Render world processing failed: {msg}"),
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
    pub fn calculate_2d_single_channel(width: u32, height: u32) -> Result<TextureSizeInfo, FogResetError> {
        let bytes_per_pixel = 1u64; // Single channel (R8Unorm)
        
        let bytes_per_row = (width as u64)
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture width too large: {width}")))?;
        
        let total_bytes = bytes_per_row
            .checked_mul(height as u64)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture size too large: {width}x{height}")))?;
        
        let total_bytes_usize = usize::try_from(total_bytes)
            .map_err(|_| FogResetError::Unknown(format!("Texture size exceeds usize: {total_bytes}")))?;
        
        let bytes_per_row_usize = usize::try_from(bytes_per_row)
            .map_err(|_| FogResetError::Unknown(format!("Bytes per row exceeds usize: {bytes_per_row}")))?;
        
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
        
        let total_bytes = bytes_per_row
            .checked_mul(height as u64)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture size too large: {width}x{height}")))?;
        
        let total_bytes_usize = usize::try_from(total_bytes)
            .map_err(|_| FogResetError::Unknown(format!("Texture size exceeds usize: {total_bytes}")))?;
        
        let bytes_per_row_usize = usize::try_from(bytes_per_row)
            .map_err(|_| FogResetError::Unknown(format!("Bytes per row exceeds usize: {bytes_per_row}")))?;
        
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
    pub fn calculate_3d_single_channel(width: u32, height: u32, depth_or_layers: u32) -> Result<TextureSizeInfo, FogResetError> {
        let bytes_per_pixel = 1u64; // Single channel (R8Unorm)
        
        let bytes_per_row = (width as u64)
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture width too large: {width}")))?;
        
        let bytes_per_slice = bytes_per_row
            .checked_mul(height as u64)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture slice too large: {width}x{height}")))?;
        
        let total_bytes = bytes_per_slice
            .checked_mul(depth_or_layers as u64)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture array too large: {width}x{height}x{depth_or_layers}")))?;
        
        let total_bytes_usize = usize::try_from(total_bytes)
            .map_err(|_| FogResetError::Unknown(format!("Texture size exceeds usize: {total_bytes}")))?;
        
        let bytes_per_row_usize = usize::try_from(bytes_per_row)
            .map_err(|_| FogResetError::Unknown(format!("Bytes per row exceeds usize: {bytes_per_row}")))?;
        
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
    pub fn calculate_3d_rgba(width: u32, height: u32, depth_or_layers: u32) -> Result<TextureSizeInfo, FogResetError> {
        let bytes_per_pixel = 4u64; // RGBA
        
        let bytes_per_row = (width as u64)
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture width too large: {width}")))?;
        
        let bytes_per_slice = bytes_per_row
            .checked_mul(height as u64)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture slice too large: {width}x{height}")))?;
        
        let total_bytes = bytes_per_slice
            .checked_mul(depth_or_layers as u64)
            .ok_or_else(|| FogResetError::Unknown(format!("Texture array too large: {width}x{height}x{depth_or_layers}")))?;
        
        let total_bytes_usize = usize::try_from(total_bytes)
            .map_err(|_| FogResetError::Unknown(format!("Texture size exceeds usize: {total_bytes}")))?;
        
        let bytes_per_row_usize = usize::try_from(bytes_per_row)
            .map_err(|_| FogResetError::Unknown(format!("Bytes per row exceeds usize: {bytes_per_row}")))?;
        
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
pub struct ResetFogOfWarEvent;

/// 事件：雾效重置成功完成
/// Event: Fog of war reset completed successfully
#[derive(Event, Debug, Default)]
pub struct FogResetSuccessEvent {
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
pub struct FogResetFailedEvent {
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
