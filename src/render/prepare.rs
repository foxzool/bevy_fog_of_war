use crate::prelude::FogMapSettings;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::render::view::ViewUniforms;
// Needed for view bindings / 视图绑定需要 // For default texture / 用于默认纹理

use super::extract::{
    ChunkComputeData, ExtractedGpuChunkData, ExtractedVisionSources, OverlayChunkData,
    RenderFogMapSettings, RenderFogTexture, RenderSnapshotTexture, VisionSourceData,
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
    render_queue: Res<RenderQueue>,
) {
    // Create buffer if it doesn't exist / 如果缓冲区不存在则创建
    let buffer = fog_uniforms.buffer.get_or_insert_with(|| {
        render_device.create_buffer(&BufferDescriptor {
            label: Some("fog_settings_uniform_buffer"),
            size: FogMapSettings::min_size().get(), // Get size from ShaderType / 从 ShaderType 获取大小
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    });

    // Write settings data to buffer / 将设置数据写入缓冲区
    let settings_data = settings.0.clone(); // Assuming FogMapSettings derives ShaderType / 假设 FogMapSettings 派生 ShaderType
    let buffer_contents = settings_data.as_wgsl_bytes(); // Convert to byte slice / 转换为字节切片
    render_queue.write_buffer(buffer, 0, &buffer_contents);
}

// Helper function to prepare dynamic storage buffers / 准备动态存储缓冲区的辅助函数
fn prepare_storage_buffer<T: ShaderType + Clone>(
    buffer_res: &mut Option<Buffer>,
    capacity: &mut usize,
    label: &str,
    data: &[T],
    render_device: &RenderDevice,
    render_queue: &RenderQueue,
) {
    let new_size = data.len() * std::mem::size_of::<T>();
    let new_capacity = data.len();

    match buffer_res {
        Some(buffer) if new_capacity <= *capacity => {
            // Buffer exists and has enough capacity, just write data / 缓冲区存在且容量足够，只需写入数据
            if new_size > 0 {
                let byte_data = data.as_wgsl_bytes();
                render_queue.write_buffer(buffer, 0, &byte_data);
            }
        }
        _ => {
            // Create or recreate buffer / 创建或重新创建缓冲区
            *buffer_res = Some(render_device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size: new_size.max(T::min_size().get()) as u64, // Ensure minimum size / 确保最小大小
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            *capacity = new_capacity;
            if new_size > 0 {
                let byte_data = data.as_wgsl_bytes();
                render_queue.write_buffer(buffer_res.as_ref().unwrap(), 0, &byte_data);
            }
            // info!("Created/Resized {} buffer, capacity: {}", label, new_capacity);
        }
    }
}

pub fn prepare_vision_source_buffer(
    extracted_sources: Res<ExtractedVisionSources>,
    mut buffer_res: ResMut<VisionSourceBuffer>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    prepare_storage_buffer(
        &mut buffer_res.buffer,
        &mut buffer_res.capacity,
        "vision_source_storage_buffer",
        &extracted_sources.sources,
        &render_device,
        &render_queue,
    );
}

pub fn prepare_gpu_chunk_buffer(
    extracted_chunks: Res<ExtractedGpuChunkData>,
    mut buffer_res: ResMut<GpuChunkInfoBuffer>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    prepare_storage_buffer(
        &mut buffer_res.buffer,
        &mut buffer_res.capacity,
        "gpu_chunk_info_storage_buffer",
        &extracted_chunks.compute_chunks,
        &render_device,
        &render_queue,
    );
}

pub fn prepare_overlay_chunk_mapping_buffer(
    extracted_chunks: Res<ExtractedGpuChunkData>,
    mut buffer_res: ResMut<OverlayChunkMappingBuffer>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    prepare_storage_buffer(
        &mut buffer_res.buffer,
        &mut buffer_res.capacity,
        "overlay_chunk_mapping_storage_buffer",
        &extracted_chunks.overlay_mapping,
        &render_device,
        &render_queue,
    );
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
        .unwrap_or(&fallback_image.texture_view);

    let snapshot_texture_view = images
        .get(&snapshot_texture.0)
        .map(|img| &img.texture_view)
        .unwrap_or(&fallback_image.texture_view);

    // --- Compute Bind Group Layout ---
    // --- 计算绑定组布局 ---
    let compute_layout = fog_bind_groups.compute_layout.get_or_insert_with(|| {
        render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("fog_compute_bind_group_layout"),
            entries: &[
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
                        min_binding_size: Some(FogMapSettings::min_size()),
                    },
                    count: None,
                },
            ],
        })
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
        render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("fog_overlay_bind_group_layout"),
            entries: &[
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
                        min_binding_size: Some(FogMapSettings::min_size()),
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
        })
    });

    // Overlay Bind Group is created per-view in the FogOverlayNode using this layout
    // 覆盖绑定组在 FogOverlayNode 中使用此布局按视图创建
}
