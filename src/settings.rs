use bevy::color::Color;
use bevy::color::palettes::basic;
use bevy::math::{IVec2, UVec2, Vec2};
use bevy::prelude::Resource;
use bevy::render::render_resource::TextureFormat;

/// The maximum number of layers allowed in the fog of war texture array.
/// 允许在雾效纹理数组中的最大层数。
pub const MAX_LAYERS: u32 = 64;

/// 战争迷雾地图的全局设置
/// Global settings for the fog of war map
#[derive(Resource, Clone, Debug)]
pub struct FogMapSettings {
    pub enabled: bool,
    // 默认区块大小 / Default chunk size
    pub chunk_size: UVec2,
    // 默认每区块分辨率 / Default resolution per chunk
    pub texture_resolution_per_chunk: UVec2,
    // 未探索区域的雾颜色 / Fog color for unexplored areas
    pub fog_color_unexplored: Color,
    // 已探索区域的雾颜色 / Fog color for explored areas
    pub fog_color_explored: Color,
    // 视野清除颜色 / Vision clear color
    pub vision_clear_color: Color,
    // 雾效纹理格式 / Fog texture format
    pub fog_texture_format: TextureFormat,
    // 快照纹理格式 / Snapshot texture format
    pub snapshot_texture_format: TextureFormat,
}

impl Default for FogMapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            chunk_size: UVec2::splat(256),
            texture_resolution_per_chunk: UVec2::splat(512),
            fog_color_unexplored: Color::BLACK,
            fog_color_explored: basic::GRAY.into(),
            vision_clear_color: Color::NONE,
            // R8Unorm 通常足够表示雾的浓度 (0.0 可见, 1.0 遮蔽)
            // R8Unorm is often sufficient for fog density (0.0 visible, 1.0 obscured)
            fog_texture_format: TextureFormat::R8Unorm,
            // 快照需要颜色和透明度 / Snapshots need color and alpha
            snapshot_texture_format: TextureFormat::Rgba8UnormSrgb,
        }
    }
}

impl FogMapSettings {
    pub fn chunk_coord_to_world(&self, chunk_coord: IVec2) -> Vec2 {
        Vec2::new(
            chunk_coord.x as f32 * self.chunk_size.x as f32,
            chunk_coord.y as f32 * self.chunk_size.y as f32,
        )
    }

    /// Converts world coordinates (Vec2) to chunk coordinates (IVec2).
    /// 将世界坐标 (Vec2) 转换为区块坐标 (IVec2)。
    pub fn world_to_chunk_coords(&self, world_pos: Vec2) -> IVec2 {
        let chunk_x = (world_pos.x / self.chunk_size.x as f32).floor() as i32;
        let chunk_y = (world_pos.y / self.chunk_size.y as f32).floor() as i32;
        IVec2::new(chunk_x, chunk_y)
    }
}
