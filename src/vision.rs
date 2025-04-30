use crate::prelude::InCameraView;
use bevy_ecs::prelude::*;
use bevy_encase_derive::ShaderType;
use bevy_math::Vec2;
use bevy_reflect::Reflect;
use bevy_render::{
    Extract,
    prelude::ViewVisibility,
    render_resource::{Buffer, BufferInitDescriptor, BufferUsages},
    renderer::RenderDevice,
};
use bevy_render_macros::ExtractComponent;
use bevy_transform::prelude::*;
use bytemuck::{Pod, Zeroable};

/// 视野提供者组件
/// Vision provider component
#[derive(Component, Reflect, ExtractComponent, Clone)]
#[require(InCameraView)]
pub struct VisionProvider {
    /// 视野范围（世界单位）
    /// Vision range (world units)
    pub range: f32,
}

// 视野源参数在 GPU 中的表示
// GPU representation of vision source parameters
#[derive(ShaderType, Pod, Zeroable, Clone, Copy, Debug)]
#[repr(C)] // Ensure C-compatible layout
pub struct GpuVisionSource {
    pub position: Vec2, // 8 bytes
    pub radius: f32,    // 4 bytes
    pub _padding: f32,  // 4 bytes padding, total 16 bytes to match WGSL
}

// 视野参数在 GPU 中的表示
// GPU representation of vision parameters
#[derive(Debug, Clone, Copy, ShaderType, Pod, Zeroable)]
#[repr(C)] // Ensure C-compatible layout
pub struct GpuVisionParams {
    // Number of vision sources (u32)
    pub count: u32,         // 4 bytes
    pub _padding: [u32; 3], // 12 bytes padding to align sources array to 16 bytes
    // Use a large fixed-size array or a dynamically sized buffer approach
    // Needs alignment considerations for WGSL
    // Example: Use Vec in staging buffer, copy to fixed-size array in GPU buffer if possible,
    pub sources: [GpuVisionSource; 16], // Example: fixed-size array of 16 sources
}

// 视野参数资源
// Vision parameters resource
#[derive(Resource, Default)]
pub struct VisionParamsResource {
    pub buffer: Option<Buffer>,
}

// 更新视野参数的 system
// System for updating vision parameters
pub fn update_vision_params(
    mut vision_params: ResMut<VisionParamsResource>,
    render_device: Res<RenderDevice>,
    query: Extract<Query<(&GlobalTransform, &VisionProvider, &ViewVisibility)>>,
) {
    let mut sources = [GpuVisionSource {
        position: Vec2::ZERO,
        radius: 0.0,
        _padding: 0.0,
    }; 16];
    let mut count = 0;
    for (transform, provider, vis) in query.iter().take(16) {
        if vis.get() {
            sources[count] = GpuVisionSource {
                position: transform.translation().truncate(),
                radius: provider.range,
                _padding: 0.2,
            };
            count += 1;
        }
    }

    let params = GpuVisionParams {
        count: count as u32,
        _padding: [0; 3],
        sources,
    };

    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("vision_params_buffer"),
        contents: bytemuck::cast_slice(&[params]),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
    });
    vision_params.buffer = Some(buffer);
}
