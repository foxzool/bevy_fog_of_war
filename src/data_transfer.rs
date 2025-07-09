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

/// 事件：重置所有雾效数据，包括已探索区域、可见性状态和纹理数据。
/// Event: Reset all fog of war data, including explored areas, visibility states, and texture data.
#[derive(Event, Debug, Default)]
pub struct ResetFogOfWarEvent;

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
    Failed(String),
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
}

/// 重置检查点，用于回滚
/// Reset checkpoint for rollback
#[derive(Debug, Clone)]
pub struct ResetCheckpoint {
    /// 探索区块数量
    /// Number of explored chunks
    pub explored_chunks_count: usize,
    /// 可见区块数量
    /// Number of visible chunks
    pub visible_chunks_count: usize,
    /// GPU驻留区块数量
    /// Number of GPU resident chunks
    pub gpu_resident_chunks_count: usize,
    /// 检查点创建时间
    /// Checkpoint creation time
    pub created_at: u64,
}

impl Default for FogResetSync {
    fn default() -> Self {
        Self {
            state: ResetSyncState::Idle,
            start_time: None,
            timeout_ms: 5000, // 5秒超时 / 5 second timeout
            checkpoint: None,
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
    pub fn mark_failed(&mut self, error: String) {
        self.state = ResetSyncState::Failed(error);
    }
    
    /// 重置到空闲状态
    /// Reset to idle state
    pub fn reset_to_idle(&mut self) {
        self.state = ResetSyncState::Idle;
        self.start_time = None;
        self.checkpoint = None;
    }
}
