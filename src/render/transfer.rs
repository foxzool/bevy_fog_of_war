use crate::prelude::*;
use crate::render::RenderFogMapSettings;
use crate::render::extract::{RenderFogTexture, RenderSnapshotTexture, RenderVisibilityTexture};
use crate::settings::MAX_LAYERS;
use async_channel::{Receiver, Sender};
use bevy::{
    image::TextureFormatPixelInfo,
    platform::collections::HashMap,
    render::MainWorld,
    render::render_asset::RenderAssets,
    render::render_resource::{
        Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, MapMode,
        Origin3d, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
        TextureFormat,
    },
    render::renderer::{RenderDevice, RenderQueue},
    render::texture::GpuImage,
};

pub fn process_cpu_to_gpu_copies(
    render_queue: Res<RenderQueue>,
    cpu_upload_requests: Res<CpuToGpuCopyRequests>,
    fog_texture_array_handle: Res<RenderFogTexture>,
    snapshot_texture_array_handle: Res<RenderSnapshotTexture>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    mut cpu_to_gpu_requests: ResMut<CpuToGpuRequests>,
    render_device: Res<RenderDevice>,
) {
    if cpu_upload_requests.requests.is_empty() {
        return;
    }

    let Some(fog_gpu_image) = gpu_images.get(&fog_texture_array_handle.0) else {
        return;
    };

    let Some(snapshot_gpu_image) = gpu_images.get(&snapshot_texture_array_handle.0) else {
        return;
    };

    for request in &cpu_upload_requests.requests {
        let mut command_encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("fog of war cpu_to_gpu_copy"),
        });
        // --- 上传雾效纹理数据 ---
        // --- Upload Fog Texture Data ---
        if let Some(upload_fog_image) = gpu_images.get(&request.fog_image_handle) {
            command_encoder.copy_texture_to_texture(
                upload_fog_image.texture.as_image_copy(),
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
                Extent3d {
                    width: upload_fog_image.size.width,
                    height: upload_fog_image.size.height,
                    depth_or_array_layers: 1,
                },
            );
        }

        // --- 上传快照纹理数据 ---
        // --- Upload Snapshot Texture Data ---
        if let Some(upload_snapshot_image) = gpu_images.get(&request.snapshot_image_handle) {
            command_encoder.copy_texture_to_texture(
                upload_snapshot_image.texture.as_image_copy(),
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
                Extent3d {
                    width: upload_snapshot_image.size.width,
                    height: upload_snapshot_image.size.height,
                    depth_or_array_layers: 1,
                },
            );
        }

        render_queue.submit(std::iter::once(command_encoder.finish()));
        cpu_to_gpu_requests.requests.push(request.chunk_coords);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn initiate_gpu_to_cpu_copies_and_request_map(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    gpu_read_requests: Res<GpuToCpuCopyRequests>,
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
                    bytes_per_row: Some(u32::try_from(bytes_per_row_fog)
                        .expect("Fog bytes per row too large for u32")), // Must be correctly aligned if required by backend
                    rows_per_image: Some(texture_height),   // For 2D, this is height
                },
            },
            Extent3d {
                width: texture_width,
                height: texture_height,
                depth_or_array_layers: 1, // Copying a single layer
            },
        );

        // 安全的清除缓冲区大小计算，防止整数溢出
        // Safe clear buffer size calculation to prevent integer overflow
        let clear_bytes_per_row_unpadded = (texture_width as usize)
            .checked_mul(TextureFormat::R8Unorm.pixel_size())
            .expect("Clear bytes per row calculation would overflow");
        let clear_padded_bytes_per_row =
            RenderDevice::align_copy_bytes_per_row(clear_bytes_per_row_unpadded);
        let clear_buffer_size = clear_padded_bytes_per_row
            .checked_mul(texture_height as usize)
            .expect("Clear buffer size calculation would overflow");

        let zero_data = vec![0u8; clear_buffer_size];
        let buffer = render_device.create_buffer_with_data(
            &bevy::render::render_resource::BufferInitDescriptor {
                label: Some("clear_fog_layer_buffer"),
                contents: &zero_data,
                usage: BufferUsages::COPY_SRC,
            },
        );

        command_encoder.copy_buffer_to_texture(
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(u32::from(
                        std::num::NonZeroU32::new(clear_padded_bytes_per_row as u32)
                            .expect("Clear buffer row size should not be zero"),
                    )),
                    rows_per_image: None,
                },
            },
            TexelCopyTextureInfo {
                // Target the correct texture (explored_read seems right for clearing render data)
                texture: &fog_gpu_image.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: request.fog_layer_index,
                },
                aspect: TextureAspect::All,
            },
            Extent3d {
                // Ensure the extent matches the area being cleared
                width: texture_width,
                height: texture_height,
                depth_or_array_layers: 1, // Clearing one layer
            },
        );

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
                    bytes_per_row: Some(u32::try_from(bytes_per_row_snapshot)
                        .expect("Snapshot bytes per row too large for u32")),
                    rows_per_image: Some(texture_height),
                },
            },
            Extent3d {
                width: texture_width,
                height: texture_height,
                depth_or_array_layers: 1, // Copying a single layer
            },
        );

        // 安全的快照清除缓冲区大小计算，防止整数溢出
        // Safe snapshot clear buffer size calculation to prevent integer overflow
        let clear_bytes_per_row_unpadded = (texture_width as usize)
            .checked_mul(TextureFormat::Rgba8Unorm.pixel_size())
            .expect("Snapshot clear bytes per row calculation would overflow");
        let clear_padded_bytes_per_row =
            RenderDevice::align_copy_bytes_per_row(clear_bytes_per_row_unpadded);
        let clear_buffer_size = clear_padded_bytes_per_row
            .checked_mul(texture_height as usize)
            .expect("Snapshot clear buffer size calculation would overflow");

        let zero_data = vec![0u8; clear_buffer_size];
        let buffer = render_device.create_buffer_with_data(
            &bevy::render::render_resource::BufferInitDescriptor {
                label: Some("clear_snap_layer_buffer"),
                contents: &zero_data,
                usage: BufferUsages::COPY_SRC,
            },
        );

        command_encoder.copy_buffer_to_texture(
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    // ***** CHANGE HERE *****
                    // Use the bytes_per_row calculated for the zero_data buffer
                    bytes_per_row: Some(u32::from(
                        std::num::NonZeroU32::new(clear_padded_bytes_per_row as u32)
                            .expect("Clear buffer row size should not be zero"),
                    )),
                    // rows_per_image should likely be None when copying to a single 2D layer/slice
                    rows_per_image: None,
                },
            },
            TexelCopyTextureInfo {
                // Target the correct texture (explored_read seems right for clearing render data)
                texture: &snapshot_gpu_image.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: request.snapshot_layer_index,
                },
                aspect: TextureAspect::All,
            },
            Extent3d {
                // Ensure the extent matches the area being cleared
                width: texture_width,
                height: texture_height,
                depth_or_array_layers: 1, // Clearing one layer
            },
        );

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
}

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
            main_world.send_event(ChunkGpuDataReadyEvent {
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

#[derive(Resource, Default)]
pub struct CpuToGpuRequests {
    pub requests: Vec<IVec2>,
}

/// 存储正在进行的 GPU 到 CPU 复制操作的状态。
/// Stores the state of ongoing GPU to CPU copy operations.
#[derive(Resource, Default)]
pub struct GpuToCpuActiveCopies {
    pub pending_copies: HashMap<IVec2, PendingCopyData>,
    pub mapped_copies: HashMap<IVec2, PendingCopyData>,
}

pub struct PendingCopyData {
    fog_buffer: Buffer,
    fog_tx: Sender<Vec<u8>>,
    fog_rx: Receiver<Vec<u8>>,
    snapshot_buffer: Buffer,
    snapshot_tx: Sender<Vec<u8>>,
    snapshot_rx: Receiver<Vec<u8>>,
    original_request: GpuToCpuCopyRequest, // To reconstruct the event later
    fog_result: Option<Vec<u8>>,
    snapshot_result: Option<Vec<u8>>,
}

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

pub(crate) fn check_cpu_to_gpu_request(
    mut main_world: ResMut<MainWorld>,
    mut cpu_to_gpu_requests: ResMut<CpuToGpuRequests>,
) {
    let requests = cpu_to_gpu_requests
        .requests
        .drain(..)
        .map(|coord| ChunkCpuDataUploadedEvent {
            chunk_coords: coord,
        })
        .collect::<Vec<ChunkCpuDataUploadedEvent>>();

    main_world.send_event_batch(requests);
}

/// 检查并清空纹理（在渲染世界中重置时）
/// Check and clear textures (when resetting in render world)
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
        reset_sync.mark_failed("Failed to get fog GPU image during reset".to_string());
        return;
    };
    let Some(visibility_gpu_image) = gpu_images.get(&visibility_texture.0) else {
        error!("Failed to get visibility GPU image during reset");
        reset_sync.mark_failed("Failed to get visibility GPU image during reset".to_string());
        return;
    };
    let Some(snapshot_gpu_image) = gpu_images.get(&snapshot_texture.0) else {
        error!("Failed to get snapshot GPU image during reset");
        reset_sync.mark_failed("Failed to get snapshot GPU image during reset".to_string());
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
    let fog_padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(fog_bytes_per_row as usize);
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
    let vis_padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(vis_bytes_per_row as usize);
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
    let snap_padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(snap_bytes_per_row as usize);
    let snap_buffer_size = snap_padded_bytes_per_row
        .checked_mul(texture_height as usize)
        .expect("Snapshot buffer size calculation would overflow");
    let snap_clear_data = vec![0u8; snap_buffer_size]; // Clear to black

    // Create reusable buffers once instead of creating new ones for each layer
    // 创建可重用缓冲区一次，而不是为每个层创建新的缓冲区
    let fog_buffer = render_device.create_buffer_with_data(
        &bevy::render::render_resource::BufferInitDescriptor {
            label: Some("fog_reset_clear_buffer"),
            contents: &fog_clear_data,
            usage: BufferUsages::COPY_SRC,
        },
    );

    let vis_buffer = render_device.create_buffer_with_data(
        &bevy::render::render_resource::BufferInitDescriptor {
            label: Some("visibility_reset_clear_buffer"),
            contents: &vis_clear_data,
            usage: BufferUsages::COPY_SRC,
        },
    );

    let snap_buffer = render_device.create_buffer_with_data(
        &bevy::render::render_resource::BufferInitDescriptor {
            label: Some("snapshot_reset_clear_buffer"),
            contents: &snap_clear_data,
            usage: BufferUsages::COPY_SRC,
        },
    );

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
                origin: Origin3d { x: 0, y: 0, z: layer },
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
                origin: Origin3d { x: 0, y: 0, z: layer },
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
                origin: Origin3d { x: 0, y: 0, z: layer },
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
    
    // 标记渲染世界处理完成
    // Mark render world processing complete
    reset_sync.mark_complete();
    info!("Render world texture reset processing complete");
}
