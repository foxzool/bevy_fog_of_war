use bevy::color::Color;
use bevy::color::palettes::basic;
use bevy::math::{IVec2, UVec2, Vec2};
use bevy::prelude::Resource;
use bevy::render::render_resource::TextureFormat;

/// The maximum number of layers allowed in the fog of war texture array.
/// 允许在雾效纹理数组中的最大层数。
pub const MAX_LAYERS: u32 = 64;

/// Global configuration resource for the fog of war system.
/// 战争迷雾地图的全局设置
///
/// This resource contains all configurable parameters that control the behavior and
/// appearance of the fog of war system. It serves as the single source of truth for
/// settings that affect chunk management, rendering, memory usage, and visual appearance.
///
/// # Configuration Categories
///
/// ## Performance Settings
/// - `chunk_size`: Affects memory usage and update granularity
/// - `texture_resolution_per_chunk`: GPU memory vs visual quality trade-off
/// - `fog_texture_format` & `snapshot_texture_format`: Memory usage optimization
///
/// ## Visual Settings  
/// - `fog_color_unexplored`: Appearance of undiscovered areas
/// - `fog_color_explored`: Appearance of previously seen areas  
/// - `vision_clear_color`: Color used for fully visible areas
///
/// ## System Control
/// - `enabled`: Master switch for the entire fog of war system
///
/// # Performance Impact
/// - **Chunk Size**: Larger chunks = fewer chunks but less granular updates
/// - **Texture Resolution**: Higher resolution = better visual quality but more GPU memory
/// - **Texture Formats**: R8 uses 1/4 the memory of RGBA8 but limits color options
///
/// # Typical Configurations
///
/// **Performance-Optimized** (mobile/low-end):
/// ```rust
/// # use bevy_fog_of_war::prelude::*;
/// # use bevy::prelude::*;
/// # use bevy::render::render_resource::TextureFormat;
/// let settings = FogMapSettings {
///     chunk_size: UVec2::splat(512),          // Larger chunks, fewer updates
///     texture_resolution_per_chunk: UVec2::splat(256), // Lower resolution  
///     fog_texture_format: TextureFormat::R8Unorm,      // 1 byte per pixel
///     snapshot_texture_format: TextureFormat::R8Unorm, // 1 byte per pixel
///     ..Default::default()
/// };
/// ```
///
/// **Quality-Optimized** (desktop/high-end):
/// ```rust
/// # use bevy_fog_of_war::prelude::*;
/// # use bevy::prelude::*;
/// # use bevy::render::render_resource::TextureFormat;
/// let settings = FogMapSettings {
///     chunk_size: UVec2::splat(128),          // Smaller chunks, more granular
///     texture_resolution_per_chunk: UVec2::splat(1024), // Higher resolution
///     fog_texture_format: TextureFormat::R8Unorm,       // Still efficient for fog
///     snapshot_texture_format: TextureFormat::Rgba8UnormSrgb, // Full color snapshots
///     ..Default::default()
/// };
/// ```
///
/// # Memory Usage Calculation
/// ```text
/// Total GPU Memory per Active Chunk =
///   (texture_resolution² × fog_format_bytes) +
///   (texture_resolution² × snapshot_format_bytes)
///
/// Example with defaults (512² × R8 + 512² × RGBA8):
///   = (262,144 × 1) + (262,144 × 4) = ~1.25 MB per chunk
/// ```
#[derive(Resource, Clone, Debug)]
pub struct FogMapSettings {
    /// Global enable/disable switch for the entire fog of war system.
    /// 是否启用雾效系统
    ///
    /// When disabled, no fog processing occurs and no fog overlay is rendered.
    /// This is useful for debugging, testing, or providing player options to disable fog.
    ///
    /// **Performance**: Disabling fog completely eliminates all fog-related CPU and GPU overhead
    /// **Use Cases**: Debug mode, accessibility options, performance testing
    pub enabled: bool,

    /// Size of each chunk in world units.
    /// 默认区块大小
    ///
    /// Determines how the world is divided into manageable spatial regions.
    /// Each chunk covers a `chunk_size.x × chunk_size.y` area of the game world.
    ///
    /// **Trade-offs**:
    /// - **Larger chunks**: Fewer total chunks, less memory overhead, coarser update granularity
    /// - **Smaller chunks**: More chunks, higher memory overhead, finer update granularity
    ///
    /// **Recommended Range**: 128-512 units per side
    /// **Default**: 256×256 units (good balance for most games)
    pub chunk_size: UVec2,

    /// Resolution of fog textures per chunk in pixels.
    /// 默认每区块分辨率
    ///
    /// Controls the pixel density of fog data within each chunk. Higher resolutions
    /// provide smoother fog edges and more detailed vision areas at the cost of GPU memory.
    ///
    /// **Memory Impact**: Each chunk uses `resolution² × bytes_per_pixel` of GPU memory
    /// **Visual Quality**: Higher resolution = smoother fog gradients and vision edges
    /// **Performance**: Higher resolution = more GPU memory bandwidth usage
    ///
    /// **Recommended Range**: 256-1024 pixels per side  
    /// **Default**: 512×512 pixels (good quality/performance balance)
    pub texture_resolution_per_chunk: UVec2,

    /// Color used to render completely unexplored areas.
    /// 未探索区域的雾颜色
    ///
    /// This color represents areas that have never been revealed by any vision source.
    /// Typically black or very dark to convey the unknown.
    ///
    /// **Alpha Channel**: Controls overall fog opacity
    /// **Common Values**: Black, dark gray, or dark blue for mystery effect
    /// **Default**: Pure black (completely obscured)
    pub fog_color_unexplored: Color,

    /// Color used to render previously explored but not currently visible areas.
    /// 已探索区域的雾颜色
    ///
    /// This color overlays areas that were once visible but are no longer in active vision.
    /// Usually lighter than unexplored fog to indicate partial knowledge.
    ///
    /// **Alpha Channel**: Controls transparency to show underlying snapshot data
    /// **Common Values**: Medium gray or colored tint matching game's art style  
    /// **Default**: Medium gray (semi-transparent overlay)
    pub fog_color_explored: Color,

    /// Color used for areas in active vision (usually transparent).
    /// 视野清除颜色
    ///
    /// This color is applied to areas currently visible to vision sources.
    /// Typically transparent to show the real game world without fog overlay.
    ///
    /// **Alpha Channel**: Usually 0.0 for complete transparency
    /// **Special Effects**: Non-transparent values can create colored vision effects
    /// **Default**: Transparent (no fog overlay in visible areas)
    pub vision_clear_color: Color,

    /// GPU texture format for real-time fog visibility data.
    /// 雾效纹理格式
    ///
    /// Determines the pixel format used to store current visibility information.
    /// This texture is continuously updated by compute shaders.
    ///
    /// **Common Formats**:
    /// - `R8Unorm`: 1 byte/pixel, grayscale only (most efficient)
    /// - `RG8Unorm`: 2 bytes/pixel, can store fog + additional data
    /// - `RGBA8UnormSrgb`: 4 bytes/pixel, full color information
    ///
    /// **Recommendation**: R8Unorm for most use cases (fog is typically grayscale)
    /// **Default**: R8Unorm (memory efficient, sufficient for fog density)
    pub fog_texture_format: TextureFormat,

    /// GPU texture format for persistent exploration snapshot data.
    /// 快照纹理格式
    ///
    /// Determines the pixel format used to store exploration history and cached
    /// information about previously visible areas.
    ///
    /// **Common Formats**:
    /// - `R8Unorm`: 1 byte/pixel, just exploration state (most memory efficient)
    /// - `RGBA8UnormSrgb`: 4 bytes/pixel, can store color information from when area was visible
    ///
    /// **Trade-off**: Color snapshots use more memory but can show richer historical information
    /// **Default**: RGBA8UnormSrgb (enables colored snapshot features)
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
    /// Converts chunk coordinates to world space position (top-left corner).
    /// 将区块坐标转换为世界空间位置（左上角）
    ///
    /// Calculates the world-space position of a chunk's top-left corner based on
    /// the configured chunk size. This is used for positioning chunks in the game world
    /// and determining spatial relationships.
    ///
    /// # Parameters
    /// - `chunk_coord`: Grid coordinates of the chunk (e.g., (0,0), (1,1), (-1,2))
    ///
    /// # Returns
    /// World position in game units of the chunk's top-left corner.
    ///
    /// # Coordinate System
    /// - Chunk (0,0) is positioned at world (0,0)
    /// - Positive X chunks extend to the right
    /// - Positive Y chunks extend upward (standard 2D convention)
    /// - Negative coordinates are fully supported
    ///
    /// # Performance
    /// - **Time Complexity**: O(1) - simple arithmetic operations
    /// - **Memory**: No allocation, operates on stack values
    /// - **Thread Safety**: Read-only operation, safe for concurrent access
    ///
    /// # Example
    /// ```rust
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let settings = FogMapSettings {
    ///     chunk_size: UVec2::new(256, 256),
    ///     ..Default::default()
    /// };
    ///
    /// // Chunk (0,0) starts at world origin
    /// assert_eq!(settings.chunk_coord_to_world(IVec2::new(0, 0)), Vec2::new(0.0, 0.0));
    ///
    /// // Chunk (1,1) starts at (256, 256)
    /// assert_eq!(settings.chunk_coord_to_world(IVec2::new(1, 1)), Vec2::new(256.0, 256.0));
    ///
    /// // Negative coordinates work too
    /// assert_eq!(settings.chunk_coord_to_world(IVec2::new(-1, 0)), Vec2::new(-256.0, 0.0));
    /// ```
    pub fn chunk_coord_to_world(&self, chunk_coord: IVec2) -> Vec2 {
        Vec2::new(
            chunk_coord.x as f32 * self.chunk_size.x as f32,
            chunk_coord.y as f32 * self.chunk_size.y as f32,
        )
    }

    /// Converts world coordinates to chunk coordinates using floor division.
    /// 将世界坐标转换为区块坐标
    ///
    /// Determines which chunk contains a given world position by dividing the world
    /// coordinates by the chunk size and flooring the result. This ensures consistent
    /// chunk assignment for all positions within a chunk's boundaries.
    ///
    /// # Parameters
    /// - `world_pos`: Position in world coordinates to find the containing chunk for
    ///
    /// # Returns
    /// Chunk coordinates (IVec2) of the chunk that contains the world position.
    ///
    /// # Algorithm
    /// Uses floor division to ensure consistent behavior across coordinate systems:
    /// - Positive coordinates: standard division and floor
    /// - Negative coordinates: floor division ensures correct chunk assignment
    /// - Boundary positions: consistently assigned to the same chunk
    ///
    /// # Performance
    /// - **Time Complexity**: O(1) - two divisions and floor operations
    /// - **Memory**: No allocation, operates on stack values
    /// - **Precision**: Uses f32 arithmetic with floor() for consistent results
    ///
    /// # Coordinate Mapping
    /// With default chunk size of 256×256:
    /// - World [0, 0] to [255.99, 255.99] → Chunk (0, 0)
    /// - World [256, 256] to [511.99, 511.99] → Chunk (1, 1)
    /// - World [-0.01, -0.01] → Chunk (-1, -1)
    /// - World [-256, -256] to [-0.01, -0.01] → Chunk (-1, -1)
    ///
    /// # Example
    /// ```rust
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let settings = FogMapSettings {
    ///     chunk_size: UVec2::new(256, 256),
    ///     ..Default::default()
    /// };
    ///
    /// // Points within chunk (0,0)
    /// assert_eq!(settings.world_to_chunk_coords(Vec2::new(0.0, 0.0)), IVec2::new(0, 0));
    /// assert_eq!(settings.world_to_chunk_coords(Vec2::new(255.9, 255.9)), IVec2::new(0, 0));
    ///
    /// // Points in chunk (1,1)  
    /// assert_eq!(settings.world_to_chunk_coords(Vec2::new(256.0, 256.0)), IVec2::new(1, 1));
    /// assert_eq!(settings.world_to_chunk_coords(Vec2::new(300.0, 400.0)), IVec2::new(1, 1));
    ///
    /// // Negative coordinates
    /// assert_eq!(settings.world_to_chunk_coords(Vec2::new(-1.0, -1.0)), IVec2::new(-1, -1));
    /// assert_eq!(settings.world_to_chunk_coords(Vec2::new(-256.0, -256.0)), IVec2::new(-1, -1));
    /// ```
    pub fn world_to_chunk_coords(&self, world_pos: Vec2) -> IVec2 {
        let chunk_x = (world_pos.x / self.chunk_size.x as f32).floor() as i32;
        let chunk_y = (world_pos.y / self.chunk_size.y as f32).floor() as i32;
        IVec2::new(chunk_x, chunk_y)
    }
}
