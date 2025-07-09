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

/// 资源：标记渲染世界需要重置纹理
/// Resource: Mark that render world needs to reset textures
#[derive(Resource, Debug, Default, Clone, ExtractResource)]
pub struct FogResetPending(pub bool);
