use crate::prelude::*;
use crate::render::RenderFogMapSettings;
use crate::render::extract::{RenderFogTexture, RenderSnapshotTexture};
use bevy::render::MainWorld;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    Extent3d, Origin3d, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect, TextureFormat,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;

// System to handle GPU -> CPU copy requests
pub fn process_gpu_to_cpu_copies(
    // world: &mut World, // If using exclusive system
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>, // Not directly for copy_texture_to_buffer, but for map_buffer
    gpu_requests: Res<GpuToCpuCopyRequests>, // Extracted from MainWorld
    fog_texture_array: Res<RenderFogTexture>, // Extracted handle
    snapshot_texture_array: Res<RenderSnapshotTexture>, // Extracted handle
    images: Res<RenderAssets<GpuImage>>,
    // mut main_world_events: EventWriter<ChunkGpuDataReadyEvent>, // To send data back
    // You'll need a way to manage staging buffers and their mapping state
) {
    // For each request in gpu_requests.requests:
    // 1. Get the GpuImage for fog_texture_array and snapshot_texture_array.
    // 2. Create a staging buffer (or get one from a pool) large enough for both textures.
    //    staging_buffer_fog = render_device.create_buffer(&BufferDescriptor { ... usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ ... });
    //    staging_buffer_snapshot = ...
    // 3. Create a CommandEncoder.
    // 4. command_encoder.copy_texture_to_buffer(
    //        ImageCopyTexture { texture: &gpu_fog_image.texture, mip_level: 0, origin: Origin3d { x:0, y:0, z: req.fog_layer_index }, aspect: TextureAspect::All },
    //        ImageCopyBuffer { buffer: &staging_buffer_fog, layout: { bytes_per_row, rows_per_image ... } },
    //        texture_resolution_per_chunk,
    //    );
    //    (Similar for snapshot texture)
    // 5. Submit command_encoder.
    // 6. staging_buffer_fog.slice(..).map_async(MapMode::Read, move |result| {
    //       if result.is_ok() {
    //           // Get data from buffer view
    //           // Send ChunkGpuDataReadyEvent { coords, fog_data, snapshot_data }
    //           // Unmap buffer
    //       }
    //    });
    // This is highly simplified. Managing async mapping and buffer states is complex.
}
pub fn process_cpu_to_gpu_copies(
    render_queue: Res<RenderQueue>,
    cpu_upload_requests: Res<CpuToGpuCopyRequests>, // Extracted from MainWorld
    fog_texture_array_handle: Res<RenderFogTexture>, // Extracted handle
    snapshot_texture_array_handle: Res<RenderSnapshotTexture>, // Extracted handle
    gpu_images: Res<RenderAssets<GpuImage>>,
    render_fog_settings: Res<RenderFogMapSettings>, // Extracted settings
    // mut main_world: ResMut<MainWorld>, // mut main_world_events: EventWriter<ChunkCpuDataUploadedEvent>,
) {
    if cpu_upload_requests.requests.is_empty() {
        return;
    }

    // 获取 GPU 图像资源
    // Get GPU image resources
    let Some(fog_gpu_image) = gpu_images.get(&fog_texture_array_handle.0) else {
        // warn!("FogTextureArray GpuImage not yet available in RenderAssets for CPU->GPU copy.");
        // 如果纹理尚未在 GPU 上创建，则无法写入。
        // If texture not yet created on GPU, cannot write to it.
        // 这通常不应该发生，因为 TextureArray 应该在 setup 时创建。
        // This typically shouldn't happen as TextureArrays should be created at setup.
        return;
    };

    let Some(snapshot_gpu_image) = gpu_images.get(&snapshot_texture_array_handle.0) else {
        // warn!("SnapshotTextureArray GpuImage not yet available in RenderAssets for CPU->GPU copy.");
        return;
    };

    // 从设置中获取纹理参数
    // Get texture parameters from settings
    let texture_width = render_fog_settings.texture_resolution_per_chunk.x;
    let texture_height = render_fog_settings.texture_resolution_per_chunk.y;
    let fog_format = fog_gpu_image.texture_format; // Should match settings.fog_texture_format
    let snapshot_format = snapshot_gpu_image.texture_format; // Should match settings.snapshot_texture_format

    for request in &cpu_upload_requests.requests {
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

        // 发送事件通知主世界上传已排队（注意：这不保证GPU已完成写入）
        // Send event to notify main world that upload has been queued (Note: this doesn't guarantee GPU has finished writing)
        // main_world.send_event(ChunkCpuDataUploadedEvent {
        //     chunk_coords: request.chunk_coords,
        // });
    }

    // 请求已处理（即使某些可能因错误而跳过），清空它们以避免下一帧重复处理
    // Requests are processed (even if some were skipped due to errors),
    // they should be cleared by the system that extracts them, or by re-initializing the resource.
    // 如果 CpuToGpuCopyRequests 是从主世界 `ExtractResource` 并且每帧都重新提取，
    // 那么在 RenderApp 中不需要显式清除。
    // If CpuToGpuCopyRequests is `ExtractResource` from MainWorld and re-extracted each frame,
    // no explicit clear is needed in RenderApp.
    // The main world system `manage_chunk_texture_transfer_system` clears its request queue before repopulating.
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
    let bytes_per_row = width * bytes_per_pixel;
    Some(bytes_per_row)
}
