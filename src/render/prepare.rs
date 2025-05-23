use crate::render::compute::FogComputePipeline;
use bevy::{
    prelude::*,
    render::render_asset::RenderAssets,
    render::render_resource::*,
    render::renderer::RenderDevice,
    render::texture::{FallbackImage, GpuImage},
};

use super::extract::{
    ExtractedGpuChunkData, ExtractedVisionSources, RenderFogMapSettings, RenderFogTexture,
    RenderVisibilityTexture,
};

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
    let capacity = extracted_sources.sources.len();
    buffer_res.capacity = capacity;
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
    let capacity = extracted_chunks.overlay_mapping.len();
    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("overlay_chunk_mapping_storage_buffer"),
        contents: bytemuck::cast_slice(&extracted_chunks.overlay_mapping),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });

    buffer_res.buffer = Some(buffer);
    buffer_res.capacity = capacity;
}

#[allow(clippy::too_many_arguments)]
pub fn prepare_fog_bind_groups(
    render_device: Res<RenderDevice>,
    mut fog_bind_groups: ResMut<FogBindGroups>,
    fog_uniforms: Res<FogUniforms>,
    vision_source_buffer: Res<VisionSourceBuffer>,
    gpu_chunk_buffer: Res<GpuChunkInfoBuffer>,
    fog_texture: Res<RenderFogTexture>,
    visibility_texture: Res<RenderVisibilityTexture>,
    images: Res<RenderAssets<GpuImage>>,
    fallback_image: Res<FallbackImage>, // For default textures / 用于默认纹理
    fog_compute_pipeline: Res<FogComputePipeline>, // For view uniform binding / 用于视图统一绑定
) {
    // Get texture views, use fallback if not loaded yet / 获取纹理视图，如果尚未加载则使用后备
    let fog_texture_view = images
        .get(&fog_texture.0)
        .map(|img| &img.texture_view)
        .unwrap_or(&fallback_image.d1.texture_view);

    let visibility_texture_view = images
        .get(&visibility_texture.0)
        .map(|img| &img.texture_view)
        .unwrap_or(&fallback_image.d1.texture_view);

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
            &fog_compute_pipeline.compute_layout,
            &BindGroupEntries::sequential((
                fog_texture_view,
                visibility_texture_view,
                source_buf.as_entire_binding(),
                chunk_buf.as_entire_binding(),
                uniform_buf.as_entire_binding(),
            )),
        );

        fog_bind_groups.compute = Some(compute_bind_group);
    }
}
