use bevy_color::Color;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use bevy_render::render_resource::Buffer;
use bevy_render_macros::ExtractComponent;

///
#[derive(Component, ExtractComponent, Clone)]
pub struct FogOfWarCamera;

/// 迷雾设置
/// Fog settings
#[derive(Component, Clone, Reflect, ExtractComponent)]
pub struct FogMaterial {
    /// 迷雾颜色
    /// Fog color
    pub color: Color,
}

impl Default for FogMaterial {
    fn default() -> Self {
        Self {
            color: Color::srgba(0.0, 0.0, 0.0, 1.0), // 黑色迷雾 / Black fog
        }
    }
}

/// Resource to hold chunk information for GPU
/// 用于保存传递给GPU的chunk信息的资源
#[derive(Resource, Default)]
pub struct GpuChunks {
    pub buffer: Option<Buffer>,
    // pub offset:  u32
}
