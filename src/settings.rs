use bevy_color::Color;
use bevy_color::palettes::basic;
use bevy_math::{IVec2, UVec2, Vec2};
use bevy_ecs::prelude::Resource;
use bevy_render::render_resource::TextureFormat;

/// The maximum number of layers allowed in the fog of war texture array.
/// 允许在雾效纹理数组中的最大层数。
pub const MAX_LAYERS: u32 = 64;

/// Global configuration resource controlling fog of war behavior and appearance.
#[derive(Resource, Clone, Debug)]
pub struct FogMapSettings {
    /// Enable/disable the entire fog of war system.
    pub enabled: bool,

    /// Size of each chunk in world units (default: 256x256).
    pub chunk_size: UVec2,

    /// Resolution of fog textures per chunk in pixels (default: 512x512).
    pub texture_resolution_per_chunk: UVec2,

    /// Color for completely unexplored areas (default: black).
    pub fog_color_unexplored: Color,

    /// Color for previously explored but not currently visible areas.
    pub fog_color_explored: Color,

    /// Color for areas in active vision (default: transparent).
    pub vision_clear_color: Color,

    /// GPU texture format for fog visibility data (default: R8Unorm).
    pub fog_texture_format: TextureFormat,

    /// GPU texture format for exploration snapshots (default: RGBA8UnormSrgb).
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
    /// Converts chunk coordinates to world position (top-left corner).
    pub fn chunk_coord_to_world(&self, chunk_coord: IVec2) -> Vec2 {
        Vec2::new(
            chunk_coord.x as f32 * self.chunk_size.x as f32,
            chunk_coord.y as f32 * self.chunk_size.y as f32,
        )
    }

    /// Converts world coordinates to chunk coordinates using floor division.
    pub fn world_to_chunk_coords(&self, world_pos: Vec2) -> IVec2 {
        let chunk_x = (world_pos.x / self.chunk_size.x as f32).floor() as i32;
        let chunk_y = (world_pos.y / self.chunk_size.y as f32).floor() as i32;
        IVec2::new(chunk_x, chunk_y)
    }
}
