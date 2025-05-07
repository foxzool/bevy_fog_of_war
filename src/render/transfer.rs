use crate::prelude::*;
use crate::render::RenderFogMapSettings;
use crate::render::extract::{RenderFogTexture, RenderSnapshotTexture};
use async_channel::{Receiver, Sender};
use bevy::platform::collections::HashMap;
use bevy::render::MainWorld;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    Buffer, BufferDescriptor, BufferUsages, CommandEncoder, CommandEncoderDescriptor, Extent3d,
    MapMode, Origin3d, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo,
    TextureAspect, TextureFormat,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;

pub fn process_cpu_to_gpu_copies(
    render_queue: Res<RenderQueue>,
    cpu_upload_requests: Res<CpuToGpuCopyRequests>, // Extracted from MainWorld
    fog_texture_array_handle: Res<RenderFogTexture>, // Extracted handle
    snapshot_texture_array_handle: Res<RenderSnapshotTexture>, // Extracted handle
    gpu_images: Res<RenderAssets<GpuImage>>,
    render_fog_settings: Res<RenderFogMapSettings>, // Extracted settings
    mut cpu_to_gpu_requests: ResMut<CpuToGpuRequests>,
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

    // 从设置中获取纹理参数
    // Get texture parameters from settings
    let texture_width = render_fog_settings.texture_resolution_per_chunk.x;
    let texture_height = render_fog_settings.texture_resolution_per_chunk.y;
    let fog_format = fog_gpu_image.texture_format; // Should match settings.fog_texture_format
    let snapshot_format = snapshot_gpu_image.texture_format; // Should match settings.snapshot_texture_format

    for request in &cpu_upload_requests.requests {
        println!("cpu to gpu {:?}", request.fog_layer_index);
        // --- 上传雾效纹理数据 ---
        // --- Upload Fog Texture Data ---
        if !request.fog_data.is_empty() {
            let bytes_per_row_fog =
                calculate_bytes_per_row(texture_width, fog_format).unwrap_or_else(|| {
                    error!(
                        "Failed to calculate bytes_per_row for fog texture format: {:?}. Skipping upload for chunk {:?}.",
                        fog_format, request.chunk_coords
                    );
                    0 // Return 0 to skip this upload attempt
                });

            if bytes_per_row_fog > 0 {
                // 验证数据大小是否匹配
                // Verify data size matches
                let expected_fog_data_size = (bytes_per_row_fog * texture_height) as usize;
                if request.fog_data.len() != expected_fog_data_size {
                    error!(
                        "Fog data size mismatch for chunk {:?}. Expected {}, got {}. Format: {:?}, Res: {}x{}. Skipping upload.",
                        request.chunk_coords,
                        expected_fog_data_size,
                        request.fog_data.len(),
                        fog_format,
                        texture_width,
                        texture_height
                    );
                } else {
                    render_queue.write_texture(
                        TexelCopyTextureInfo {
                            texture: &fog_gpu_image.texture,
                            mip_level: 0,
                            origin: Origin3d {
                                x: 0,
                                y: 0,
                                z: request.fog_layer_index, // 目标层索引 Target layer index
                            },
                            aspect: TextureAspect::All, // 通常是 All，除非是深度/模板纹理 Specific to depth/stencil if they are separate
                        },
                        &request.fog_data,
                        TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(bytes_per_row_fog),
                            rows_per_image: Some(texture_height), // 对于2D纹理数组，这应该是纹理的高度 For 2D texture arrays, this should be the texture height
                        },
                        Extent3d {
                            width: texture_width,
                            height: texture_height,
                            depth_or_array_layers: 1, // 我们一次只写入一个层 We are writing to a single layer at a time
                        },
                    );
                    // trace!(
                    //     "Queued fog texture write for chunk {:?}, layer {}, size {}",
                    //     request.chunk_coords,
                    //     request.fog_layer_index,
                    //     request.fog_data.len()
                    // );
                }
            }
        } else {
            warn!(
                "Fog data for chunk {:?} is empty, skipping fog upload.",
                request.chunk_coords
            );
        }

        // --- 上传快照纹理数据 ---
        // --- Upload Snapshot Texture Data ---
        if !request.snapshot_data.is_empty() {
            let bytes_per_row_snapshot =
                calculate_bytes_per_row(texture_width, snapshot_format).unwrap_or_else(|| {
                    error!(
                        "Failed to calculate bytes_per_row for snapshot texture format: {:?}. Skipping upload for chunk {:?}.",
                        snapshot_format, request.chunk_coords
                    );
                    0
                });

            if bytes_per_row_snapshot > 0 {
                let expected_snapshot_data_size =
                    (bytes_per_row_snapshot * texture_height) as usize;
                if request.snapshot_data.len() != expected_snapshot_data_size {
                    error!(
                        "Snapshot data size mismatch for chunk {:?}. Expected {}, got {}. Format: {:?}, Res: {}x{}. Skipping upload.",
                        request.chunk_coords,
                        expected_snapshot_data_size,
                        request.snapshot_data.len(),
                        snapshot_format,
                        texture_width,
                        texture_height
                    );
                } else {
                    render_queue.write_texture(
                        TexelCopyTextureInfo {
                            texture: &snapshot_gpu_image.texture,
                            mip_level: 0,
                            origin: Origin3d {
                                x: 0,
                                y: 0,
                                z: request.snapshot_layer_index, // 目标层索引 Target layer index
                            },
                            aspect: TextureAspect::All,
                        },
                        &request.snapshot_data,
                        TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(bytes_per_row_snapshot),
                            rows_per_image: Some(texture_height),
                        },
                        Extent3d {
                            width: texture_width,
                            height: texture_height,
                            depth_or_array_layers: 1, // 我们一次只写入一个层 We are writing to a single layer at a time
                        },
                    );
                    // trace!(
                    //     "Queued snapshot texture write for chunk {:?}, layer {}, size {}",
                    //     request.chunk_coords,
                    //     request.snapshot_layer_index,
                    //     request.snapshot_data.len()
                    // );
                }
            }
        } else {
            warn!(
                "Snapshot data for chunk {:?} is empty, skipping snapshot upload.",
                request.chunk_coords
            );
        }

        cpu_to_gpu_requests.requests.push(request.chunk_coords);
    }
}

/// 辅助函数：计算每行字节数。
/// Helper function: Calculate bytes per row.
/// 返回 `Option<u32>`，因为某些格式可能不受支持或难以计算。
/// Returns `Option<u32>` as some formats might not be supported or easily calculable.
fn calculate_bytes_per_row(width: u32, format: TextureFormat) -> Option<u32> {
    let bits_per_pixel = match format {
        TextureFormat::R8Unorm
        | TextureFormat::R8Snorm
        | TextureFormat::R8Uint
        | TextureFormat::R8Sint => 8,
        TextureFormat::R16Uint
        | TextureFormat::R16Sint
        | TextureFormat::R16Unorm
        | TextureFormat::R16Snorm
        | TextureFormat::R16Float => 16,
        TextureFormat::Rg8Unorm
        | TextureFormat::Rg8Snorm
        | TextureFormat::Rg8Uint
        | TextureFormat::Rg8Sint => 16,
        TextureFormat::R32Uint | TextureFormat::R32Sint | TextureFormat::R32Float => 32,
        TextureFormat::Rg16Uint
        | TextureFormat::Rg16Sint
        | TextureFormat::Rg16Unorm
        | TextureFormat::Rg16Snorm
        | TextureFormat::Rg16Float => 32,
        TextureFormat::Rgba8Unorm
        | TextureFormat::Rgba8UnormSrgb
        | TextureFormat::Rgba8Snorm
        | TextureFormat::Rgba8Uint
        | TextureFormat::Rgba8Sint => 32,
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => 32,
        TextureFormat::Rg32Uint | TextureFormat::Rg32Sint | TextureFormat::Rg32Float => 64,
        TextureFormat::Rgba16Uint
        | TextureFormat::Rgba16Sint
        | TextureFormat::Rgba16Unorm
        | TextureFormat::Rgba16Snorm
        | TextureFormat::Rgba16Float => 64,
        TextureFormat::Rgba32Uint | TextureFormat::Rgba32Sint | TextureFormat::Rgba32Float => 128,
        _ => {
            warn!(
                "Unsupported texture format for bytes_per_row calculation: {:?}",
                format
            );
            return None;
        }
    };
    let bytes_per_pixel = bits_per_pixel / 8;
    let unaligned = width * bytes_per_pixel;
    // Align up to 256 bytes (COPY_BYTES_PER_ROW_ALIGNMENT)
    // 向上对齐到 256 字节
    let align = 256;
    let aligned = ((unaligned + align - 1) / align) * align;
    Some(aligned)
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

        let Some(bytes_per_row_fog) = calculate_bytes_per_row(texture_width, fog_format) else {
            continue;
        };
        let fog_buffer_size = (bytes_per_row_fog * texture_height) as u64;
        if fog_buffer_size == 0 {
            error!("Fog buffer size is 0 for chunk {:?}", request.chunk_coords);
            continue;
        }

        let Some(bytes_per_row_snapshot) = calculate_bytes_per_row(texture_width, snapshot_format)
        else {
            continue;
        };
        let snapshot_buffer_size = (bytes_per_row_snapshot * texture_height) as u64;
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
                    bytes_per_row: Some(bytes_per_row_fog), // Must be correctly aligned if required by backend
                    rows_per_image: Some(texture_height),   // For 2D, this is height
                },
            },
            Extent3d {
                width: texture_width,
                height: texture_height,
                depth_or_array_layers: 1, // Copying a single layer
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
                    bytes_per_row: Some(bytes_per_row_snapshot),
                    rows_per_image: Some(texture_height),
                },
            },
            Extent3d {
                width: texture_width,
                height: texture_height,
                depth_or_array_layers: 1, // Copying a single layer
            },
        );
        let (fog_tx, fog_rx) = async_channel::bounded(1);
        let (snapshot_tx, snapshot_rx) = async_channel::bounded(1);

        active_copies.pending_copies.insert(
            request.chunk_coords,
            PendingCopyData {
                fog_buffer: fog_staging_buffer,
                fog_buffer_size,
                fog_tx,
                fog_rx,
                snapshot_buffer: snapshot_staging_buffer,
                snapshot_buffer_size,
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
    fog_buffer_size: u64, // Bytes
    fog_tx: Sender<Vec<u8>>,
    fog_rx: Receiver<Vec<u8>>,
    snapshot_buffer: Buffer,
    snapshot_buffer_size: u64, // Bytes
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
