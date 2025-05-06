use bevy::image::TextureFormatPixelInfo;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::TextureFormat::R8Unorm;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::render::view::{ViewUniform, ViewUniforms};
// Needed for view bindings / 视图绑定需要 // For default texture / 用于默认纹理

use super::extract::{
    ChunkComputeData, ExtractedCpuToGpuCopyRequests, ExtractedGpuChunkData,
    ExtractedGpuToCpuCopyRequests, ExtractedVisionSources, OverlayChunkData, RenderFogMapSettings,
    RenderFogTexture, RenderSnapshotTexture, VisionSourceData,
};
use super::{FOG_COMPUTE_SHADER_HANDLE, FOG_OVERLAY_SHADER_HANDLE}; // Import shader handles / 导入 shader 句柄

// --- Resources to hold GPU buffers and bind groups ---
// --- 用于保存 GPU 缓冲区和绑定组的资源 ---

#[derive(Resource, Default)]
pub struct FogUniforms {
    pub buffer: Option<Buffer>,
}

#[derive(Resource, Default)]
pub struct VisionSourceBuffer {
    pub buffer: Option<Buffer>,
    pub capacity: usize,
}

#[derive(Resource, Default)]
pub struct GpuChunkInfoBuffer {
    pub buffer: Option<Buffer>,
    pub capacity: usize,
}

#[derive(Resource, Default)]
pub struct OverlayChunkMappingBuffer {
    pub buffer: Option<Buffer>,
    pub capacity: usize,
}

#[derive(Resource, Default)]
pub struct FogBindGroups {
    pub compute: Option<BindGroup>,
    // Overlay bind group might depend on view, handled in node or pipeline
    // 覆盖绑定组可能依赖于视图，在节点或管线中处理
    // pub overlay: Option<BindGroup>,
    pub overlay_layout: Option<BindGroupLayout>, // Store layout for pipeline / 存储布局用于管线
    pub compute_layout: Option<BindGroupLayout>, // Store layout for pipeline / 存储布局用于管线
}

// --- Buffer Preparation Systems ---
// --- 缓冲区准备系统 ---

pub fn prepare_fog_uniforms(
    settings: Res<RenderFogMapSettings>,
    mut fog_uniforms: ResMut<FogUniforms>,
    render_device: Res<RenderDevice>,
) {
    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("fog setting data buffer"),
        contents: bytemuck::cast_slice(&[*settings]),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    fog_uniforms.buffer = Some(buffer);
}

pub fn prepare_vision_source_buffer(
    extracted_sources: Res<ExtractedVisionSources>,
    mut buffer_res: ResMut<VisionSourceBuffer>,
    render_device: Res<RenderDevice>,
) {
    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("vision_source_storage_buffer"),
        contents: bytemuck::cast_slice(&extracted_sources.sources),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });

    buffer_res.buffer = Some(buffer);
}

pub fn prepare_gpu_chunk_buffer(
    extracted_chunks: Res<ExtractedGpuChunkData>,
    mut buffer_res: ResMut<GpuChunkInfoBuffer>,
    render_device: Res<RenderDevice>,
) {
    buffer_res.capacity = extracted_chunks.compute_chunks.len();
    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("gpu_chunk_info_storage_buffer"),
        contents: bytemuck::cast_slice(&extracted_chunks.compute_chunks),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });

    buffer_res.buffer = Some(buffer);
}

pub fn prepare_overlay_chunk_mapping_buffer(
    extracted_chunks: Res<ExtractedGpuChunkData>,
    mut buffer_res: ResMut<OverlayChunkMappingBuffer>,
    render_device: Res<RenderDevice>,
) {
    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("overlay_chunk_mapping_storage_buffer"),
        contents: bytemuck::cast_slice(&extracted_chunks.overlay_mapping),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });

    buffer_res.buffer = Some(buffer);
}

// --- Bind Group Preparation ---
// --- 绑定组准备 ---

pub fn prepare_fog_bind_groups(
    render_device: Res<RenderDevice>,
    mut fog_bind_groups: ResMut<FogBindGroups>,
    fog_uniforms: Res<FogUniforms>,
    vision_source_buffer: Res<VisionSourceBuffer>,
    gpu_chunk_buffer: Res<GpuChunkInfoBuffer>,
    overlay_chunk_buffer: Res<OverlayChunkMappingBuffer>,
    fog_texture: Res<RenderFogTexture>,
    snapshot_texture: Res<RenderSnapshotTexture>,
    images: Res<RenderAssets<GpuImage>>,
    fallback_image: Res<FallbackImage>, // For default textures / 用于默认纹理
    view_uniforms: Res<ViewUniforms>,   // For view uniform binding / 用于视图统一绑定
) {
    // Get texture views, use fallback if not loaded yet / 获取纹理视图，如果尚未加载则使用后备
    let fog_texture_view = images
        .get(&fog_texture.0)
        .map(|img| &img.texture_view)
        .unwrap_or(&fallback_image.d1.texture_view);

    // --- Compute Bind Group Layout ---
    // --- 计算绑定组布局 ---
    let compute_layout = fog_bind_groups.compute_layout.get_or_insert_with(|| {
        render_device.create_bind_group_layout(
            "fog_compute_bind_group_layout",
            &[
                // Fog Texture (Storage) / 雾效纹理 (存储)
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadWrite, // Read and write fog / 读写雾效
                        format: TextureFormat::R8Unorm, // Must match image format / 必须匹配图像格式
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                // Vision Sources (Storage Buffer) / 视野源 (存储缓冲区)
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(VisionSourceData::min_size()),
                    },
                    count: None,
                },
                // GPU Chunk Info (Storage Buffer) / GPU 区块信息 (存储缓冲区)
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(ChunkComputeData::min_size()),
                    },
                    count: None,
                },
                // Fog Settings (Uniform Buffer) / 雾设置 (统一缓冲区)
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE | ShaderStages::FRAGMENT, // Also used by overlay / 覆盖也会使用
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(RenderFogMapSettings::min_size()),
                    },
                    count: None,
                },
            ],
        )
    });

    // --- Compute Bind Group ---
    // --- 计算绑定组 ---
    // Only create if buffers are ready / 仅当缓冲区准备就绪时创建
    if let (Some(uniform_buf), Some(source_buf), Some(chunk_buf)) = (
        fog_uniforms.buffer.as_ref(),
        vision_source_buffer.buffer.as_ref(),
        gpu_chunk_buffer.buffer.as_ref(),
    ) {
        let compute_bind_group = render_device.create_bind_group(
            "fog_compute_bind_group",
            compute_layout,
            &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(fog_texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: source_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: chunk_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: uniform_buf.as_entire_binding(),
                },
            ],
        );
        fog_bind_groups.compute = Some(compute_bind_group);
    }

    // --- Overlay Bind Group Layout ---
    // --- 覆盖绑定组布局 ---
    // This layout is often shared or derived from a standard pipeline (like 2D)
    // 这个布局通常是共享的或从标准管线 (如 2D) 派生的
    fog_bind_groups.overlay_layout.get_or_insert_with(|| {
        render_device.create_bind_group_layout(
            "fog_overlay_bind_group_layout",
            &[
                // View Uniforms (Standard Bevy Binding) / 视图统一变量 (标准 Bevy 绑定)
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true, // Important for view uniforms / 对视图统一变量很重要
                        min_binding_size: Some(ViewUniform::min_size()),
                    },
                    count: None,
                },
                // Fog Texture (Sampled) / 雾效纹理 (采样)
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false }, // R8Unorm is not filterable / R8Unorm 不可过滤
                        view_dimension: TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // Snapshot Texture (Sampled) / 快照纹理 (采样)
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true }, // RGBA8 is filterable / RGBA8 可过滤
                        view_dimension: TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler / 采样器
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering), // Use filtering for snapshot / 对快照使用过滤
                    count: None,
                },
                // Fog Settings (Uniform Buffer) / 雾设置 (统一缓冲区) - Reuse binding 3 from compute layout? No, use new binding.
                // 重用计算布局中的绑定 3？不，使用新绑定。
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::FRAGMENT, // Only fragment needed here / 这里只需要片段
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(RenderFogMapSettings::min_size()),
                    },
                    count: None,
                },
                // Overlay Chunk Mapping (Storage Buffer) / 覆盖区块映射 (存储缓冲区)
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(OverlayChunkData::min_size()),
                    },
                    count: None,
                },
            ],
        )
    });

    // Overlay Bind Group is created per-view in the FogOverlayNode using this layout
    // 覆盖绑定组在 FogOverlayNode 中使用此布局按视图创建
}

pub fn process_texture_copies(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    images: Res<RenderAssets<GpuImage>>, // Bevy 0.12+
    // images: Res<RenderAssets<Image>>, // Bevy < 0.12
    fog_map_settings: Res<RenderFogMapSettings>, // 提取的设置 / Extracted settings
    fog_texture_array_handle: Option<Res<RenderFogTexture>>, // 使用 Option 以防尚未初始化 / Use Option in case not yet initialized
    snapshot_texture_array_handle: Option<Res<RenderSnapshotTexture>>,
    mut cpu_to_gpu_requests: ResMut<ExtractedCpuToGpuCopyRequests>,
    mut gpu_to_cpu_requests: ResMut<ExtractedGpuToCpuCopyRequests>,
) {
    // --- 1. 处理 CPU -> GPU 的复制请求 ---
    // --- 1. Process CPU -> GPU copy requests ---
    if let (Some(fog_handle_res), Some(snapshot_handle_res)) = (
        fog_texture_array_handle.as_deref(),
        snapshot_texture_array_handle.as_deref(),
    ) {
        if let (Some(fog_gpu_image), Some(snapshot_gpu_image)) = (
            images.get(&fog_handle_res.0),
            images.get(&snapshot_handle_res.0),
        ) {
            for request in cpu_to_gpu_requests.0.iter() {
                // --- 复制雾效数据 ---
                // --- Copy Fog Data ---
                if !request.fog_data.is_empty() {
                    let bytes_per_pixel_fog = R8Unorm.pixel_size(); // e.g., 1 for R8Unorm
                    if bytes_per_pixel_fog == 0 {
                        error!("Fog texture format pixel size is zero, cannot copy.");
                        continue;
                    }
                    let bytes_per_row_fog = fog_map_settings.texture_resolution_per_chunk.x
                        * bytes_per_pixel_fog as u32;
                    // 确保 bytes_per_row 是 256 的倍数 (wgpu 要求)
                    // Ensure bytes_per_row is a multiple of 256 (wgpu requirement)
                    let aligned_bytes_per_row_fog = (bytes_per_row_fog + 255) & !255;

                    render_queue.write_texture(
                        TexelCopyTextureInfo {
                            texture: &fog_gpu_image.texture,
                            mip_level: 0,
                            origin: Origin3d {
                                x: 0,
                                y: 0,
                                z: request.fog_layer_index, // 目标层索引 / Target layer index
                            },
                            aspect: TextureAspect::All, // 通常是 All / Usually All
                        },
                        &request.fog_data, // 原始字节数据 / Raw byte data
                        TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(aligned_bytes_per_row_fog),
                            rows_per_image: Some(fog_map_settings.texture_resolution_per_chunk.y),
                        },
                        Extent3d {
                            width: fog_map_settings.texture_resolution_per_chunk.x,
                            height: fog_map_settings.texture_resolution_per_chunk.y,
                            depth_or_array_layers: 1, // 只复制一个层 / Copying a single layer
                        },
                    );
                    // info!(
                    //     "Copied fog data for chunk {:?} to GPU layer {}",
                    //     request.coords, request.fog_layer_index
                    // );
                }

                // --- 复制快照数据 ---
                // --- Copy Snapshot Data ---
                if !request.snapshot_data.is_empty() {
                    let bytes_per_pixel_snapshot = snapshot_gpu_image.texture_format.pixel_size(); // e.g., 4 for Rgba8UnormSrgb
                    if bytes_per_pixel_snapshot == 0 {
                        error!("Snapshot texture format pixel size is zero, cannot copy.");
                        continue;
                    }
                    let bytes_per_row_snapshot = fog_map_settings.texture_resolution_per_chunk.x
                        * bytes_per_pixel_snapshot as u32;
                    // 确保 bytes_per_row 是 256 的倍数
                    // Ensure bytes_per_row is a multiple of 256
                    let aligned_bytes_per_row_snapshot = (bytes_per_row_snapshot + 255) & !255;

                    render_queue.write_texture(
                        TexelCopyTextureInfo {
                            texture: &snapshot_gpu_image.texture,
                            mip_level: 0,
                            origin: Origin3d {
                                x: 0,
                                y: 0,
                                z: request.snapshot_layer_index, // 目标层索引 / Target layer index
                            },
                            aspect: TextureAspect::All,
                        },
                        &request.snapshot_data,
                        TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(aligned_bytes_per_row_snapshot),
                            rows_per_image: Some(fog_map_settings.texture_resolution_per_chunk.y),
                        },
                        Extent3d {
                            width: fog_map_settings.texture_resolution_per_chunk.x,
                            height: fog_map_settings.texture_resolution_per_chunk.y,
                            depth_or_array_layers: 1,
                        },
                    );
                    // info!(
                    //     "Copied snapshot data for chunk {:?} to GPU layer {}",
                    //     request.coords, request.snapshot_layer_index
                    // );
                }
            }
        } else {
            warn!("Fog texture and Snapshot texture not available");
        }
    } else {
        // warn!("Texture array handles not yet available for CPU->GPU copy.");
    }
    cpu_to_gpu_requests.0.clear(); // 清除已处理的请求 / Clear processed requests

    // --- 2. 处理 GPU -> CPU 的复制请求 (简化版 - 仅记录) ---
    // --- 2. Process GPU -> CPU copy requests (Simplified - logging only) ---
    // 实际实现需要暂存缓冲区和异步回读
    // Actual implementation requires staging buffers and asynchronous readback
    if !gpu_to_cpu_requests.0.is_empty() {
        if let (Some(fog_handle_res), Some(snapshot_handle_res)) = (
            fog_texture_array_handle.as_deref(),
            snapshot_texture_array_handle.as_deref(),
        ) {
            if let (Some(fog_gpu_image), Some(snapshot_gpu_image)) = (
                images.get(&fog_handle_res.0),
                images.get(&snapshot_handle_res.0),
            ) {
                let mut command_encoder = render_device.create_command_encoder(&Default::default()); // 需要一个命令编码器 / Need a command encoder

                for request in gpu_to_cpu_requests.0.iter() {
                    // TODO: 实现 GPU -> CPU 的实际复制逻辑
                    // This involves:
                    // 1. Creating a staging buffer (BufferUsages::COPY_DST | BufferUsages::MAP_READ)
                    //    with size matching the texture layer data.
                    // 2. Using command_encoder.copy_texture_to_buffer(...) for fog texture.
                    // 3. Using command_encoder.copy_texture_to_buffer(...) for snapshot texture.
                    // 4. Submitting the command encoder via render_queue.submit(std::iter::once(command_encoder.finish())).
                    // 5. In a LATER frame/system (after GPU has processed the copy):
                    //    - Slicing the buffer (buffer.slice(..)).
                    //    - Calling map_async(MapMode::Read, ...).
                    //    - Polling render_device.poll(Maintain::Wait) or using a callback.
                    //    - Once mapped, get_mapped_range() to get the data.
                    //    - Send data back to MainWorld (e.g., via crossbeam-channel).
                    //    - Unmap the buffer.
                    //    - Free the staging buffer.

                    // 简化版：仅打印日志
                    // Simplified: Just log
                    // info!(
                    //     "GPU->CPU: Requested offload for chunk {:?} (Fog Layer: {}, Snapshot Layer: {}). Actual copy NOT YET IMPLEMENTED.",
                    //     request.coords, request.fog_layer_index, request.snapshot_layer_index
                    // );

                    // 示例：为雾效纹理层创建复制命令 (未完成)
                    // Example: Creating copy command for fog texture layer (incomplete)
                    let fog_bytes_per_pixel = TextureFormat::R8Unorm.pixel_size();
                    if fog_bytes_per_pixel > 0 {
                        let fog_data_size = (fog_map_settings.texture_resolution_per_chunk.x
                            * fog_map_settings.texture_resolution_per_chunk.y
                            * fog_bytes_per_pixel as u32)
                            as u64;

                        // let staging_buffer_fog = render_device.create_buffer(&BufferDescriptor {
                        //     label: Some(&format!("fog_staging_buffer_{:?}", request.coords)),
                        //     size: fog_data_size,
                        //     usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                        //     mapped_at_creation: false,
                        // });
                        //
                        // let bytes_per_row_fog = fog_map_settings.texture_resolution_per_chunk.x * fog_bytes_per_pixel as u32;
                        // let aligned_bytes_per_row_fog = (bytes_per_row_fog + 255) & !255;
                        //
                        // command_encoder.copy_texture_to_buffer(
                        //     ImageCopyTexture {
                        //         texture: &fog_gpu_image.texture,
                        //         mip_level: 0,
                        //         origin: Origin3d { x: 0, y: 0, z: request.fog_layer_index },
                        //         aspect: TextureAspect::All,
                        //     },
                        //     ImageCopyBuffer {
                        //         buffer: &staging_buffer_fog,
                        //         layout: ImageDataLayout {
                        //             offset: 0,
                        //             bytes_per_row: Some(aligned_bytes_per_row_fog),
                        //             rows_per_image: Some(fog_map_settings.texture_resolution_per_chunk.y),
                        //         },
                        //     },
                        //     Extent3d {
                        //         width: fog_map_settings.texture_resolution_per_chunk.x,
                        //         height: fog_map_settings.texture_resolution_per_chunk.y,
                        //         depth_or_array_layers: 1,
                        //     },
                        // );
                        // 在这里，你需要存储 staging_buffer_fog 和请求信息，以便稍后轮询和读取
                        // Here you would need to store staging_buffer_fog and request info for later polling and reading
                    }
                    // 对快照纹理执行类似操作
                    // Similar operation for snapshot texture
                }
                // 如果有命令被编码，则提交
                // If any commands were encoded, submit them
                // render_queue.submit(std::iter::once(command_encoder.finish()));
            }
        }
        gpu_to_cpu_requests.0.clear(); // 清除已（尝试）处理的请求 / Clear (attempted) processed requests
    }
}
