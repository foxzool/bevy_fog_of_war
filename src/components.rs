//! # Fog of War ECS Components
//!
//! This module defines all the Entity-Component-System (ECS) components used by the fog of war plugin.
//! These components represent the core data structures for vision sources, chunk management,
//! visibility states, and memory management.
//!
//! ## Component Categories
//!
//! ### Vision Components
//! - [`FogOfWarCamera`] - Marks cameras that should render fog of war
//! - [`VisionSource`] - Entities that reveal fog (units, buildings, etc.)
//! - [`Capturable`] - Entities that become visible when discovered
//!
//! ### Chunk Components  
//! - [`FogChunk`] - Represents a spatial region with fog data
//! - [`FogChunkImage`] - Image handles for chunk textures
//! - [`ChunkState`] - Aggregated visibility and memory state
//!
//! ### State Enums
//! - [`VisionShape`] - Different vision area shapes (circle, cone, square)
//! - [`ChunkVisibility`] - Exploration states (unexplored, explored, visible)
//! - [`ChunkMemoryLocation`] - Where chunk data is stored (CPU, GPU, pending)
//!
//! ## Performance Characteristics
//!
//! All components are designed for optimal ECS performance:
//! - **Small Memory Footprint**: Components use primitive types where possible
//! - **Component Reflection**: Full Bevy reflection support for editor integration
//! - **Extract Components**: GPU-accessible components marked with `ExtractComponent`
//! - **Serialization**: State components support serde for persistence

use crate::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor, TextureFormatPixelInfo};
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureUsages};
use std::fmt::Display;

/// Camera marker component for fog of war rendering.
/// 摄像机组件
///
/// This component should be added to any camera entity that should display the fog of war effect.
/// The fog of war system uses this marker to identify which cameras need fog overlay rendering
/// and to calculate the camera's view area for chunk management optimization.
///
/// # Usage
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_fog_of_war::prelude::*;
/// # fn setup(mut commands: Commands) {
/// commands.spawn((
///     Camera2d,
///     FogOfWarCamera, // Enable fog of war for this camera
/// ));
/// # }
/// ```
///
/// # Performance Impact
/// - **Overhead**: Minimal - marker component with no data
/// - **Rendering**: Adds fog overlay pass to camera's render pipeline
/// - **Chunk Culling**: Enables view-based chunk optimization
///
/// # Compatibility
/// Works with any Bevy camera type:
/// - `Camera2d` for 2D games (primary use case)
/// - `Camera3d` for 3D games with 2D fog overlay
/// - Multiple cameras supported with independent fog states
#[derive(Component)]
pub struct FogOfWarCamera;

/// Vision source component that reveals fog of war in a specified area.
/// 视野源组件
///
/// Entities with this component act as "light sources" that reveal areas of the map,
/// creating visibility for players. Vision sources support multiple shapes and can be
/// dynamically enabled/disabled, moved, and reconfigured during gameplay.
///
/// # Vision Shapes
/// - **Circle**: Omnidirectional vision (most common for units)
/// - **Cone**: Directional vision with angle (spotlights, unit facing)  
/// - **Square**: Rectangular vision area (buildings, watchtowers)
///
/// # Performance Characteristics
/// - **Complexity**: O(C) where C = chunks intersecting vision area
/// - **GPU Transfer**: Component data extracted to render world each frame
/// - **Memory**: ~48 bytes per vision source (small, cache-friendly)
///
/// # Usage Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_fog_of_war::prelude::*;
/// # fn setup(mut commands: Commands) {
/// // Simple circular vision
/// commands.spawn((
///     Transform::from_xyz(100.0, 200.0, 0.0),
///     VisionSource::circle(150.0), // 150 unit radius
/// ));
///
/// // Directional cone vision (like a flashlight)
/// commands.spawn((
///     Transform::from_xyz(0.0, 0.0, 0.0),
///     VisionSource::cone(
///         200.0,                    // range
///         0.0,                      // direction (facing right)
///         std::f32::consts::PI / 3.0 // 60 degree angle
///     ),
/// ));
///
/// // Square watchtower vision
/// commands.spawn((
///     Transform::from_xyz(500.0, 500.0, 0.0),
///     VisionSource::square(300.0),
/// ));
/// # }
/// ```
#[derive(Component, Reflect, ExtractComponent, Clone)]
#[reflect(Component)]
pub struct VisionSource {
    /// Vision range in world units (radius for circle/cone, half-width for square).
    /// 视野范围（世界单位）
    ///
    /// Determines how far the vision extends from the entity's position.
    /// For circles and cones, this is the radius. For squares, this is the
    /// half-width (distance from center to edge).
    ///
    /// **Range**: Typically 50.0 - 1000.0 world units
    /// **Performance**: Larger ranges check more chunks (O(R²) complexity)
    pub range: f32,

    /// Whether this vision source is currently active.
    /// 是否启用
    ///
    /// Disabled vision sources are completely ignored during visibility calculations,
    /// providing an efficient way to temporarily disable vision without removing
    /// the component.
    ///
    /// **Performance**: O(1) - Disabled sources are skipped immediately
    pub enabled: bool,

    /// The geometric shape of the vision area.
    /// 视野形状（默认为圆形）
    ///
    /// Determines how the vision area is calculated relative to the entity's position.
    /// Each shape has different performance characteristics and use cases.
    pub shape: VisionShape,

    /// Direction the vision is facing (radians, 0 = positive X axis).
    /// 视野方向（弧度，仅在扇形视野时使用）
    ///
    /// Only used for cone-shaped vision. Determines the center direction of the cone.
    /// - 0.0 = facing right (+X)
    /// - π/2 = facing up (+Y)  
    /// - π = facing left (-X)
    /// - 3π/2 = facing down (-Y)
    ///
    /// **Range**: 0.0 to 2π radians
    /// **Ignored**: For Circle and Square shapes
    pub direction: f32,

    /// Cone vision angle in radians (total angle, not half-angle).
    /// 视野角度（弧度，仅在扇形视野时使用）
    ///
    /// For cone vision, this defines the total angular width of the visible area.
    /// The cone extends from (direction - angle/2) to (direction + angle/2).
    ///
    /// **Common Values**:
    /// - π/6 (30°) = Narrow spotlight
    /// - π/3 (60°) = Standard unit vision
    /// - π/2 (90°) = Wide area coverage
    /// - π (180°) = Semicircle
    ///
    /// **Range**: 0.0 to 2π radians
    /// **Ignored**: For Circle and Square shapes
    pub angle: f32,

    /// Vision intensity multiplier for visibility calculations.
    /// 视野强度（影响可见性计算）
    ///
    /// Affects how "strongly" this vision source reveals fog. Higher values
    /// can penetrate through areas with partial visibility or fog density.
    ///
    /// **Range**: 0.0 to 2.0 (values > 1.0 for enhanced vision)
    /// **Default**: 1.0 (normal visibility)
    /// **Performance**: Minimal impact on calculation cost
    pub intensity: f32,

    /// Transition zone ratio for smooth fog edges (0.0 to 1.0).
    /// 视野过渡比例（从完全可见到不可见的过渡区域占总半径的比例）
    ///
    /// Defines the size of the soft transition area at the edge of vision.
    /// The transition zone creates smooth fog falloff instead of hard edges.
    ///
    /// **Calculation**: transition_distance = range × transition_ratio
    /// **Values**:
    /// - 0.0 = Hard edge (no transition)
    /// - 0.1 = Subtle soft edge (10% of range)
    /// - 0.2 = Standard soft edge (20% of range)
    /// - 0.5 = Large transition zone (50% of range)
    ///
    /// **Performance**: Slightly increases GPU shader complexity
    pub transition_ratio: f32,
}

impl VisionSource {
    /// Creates a circular (omnidirectional) vision source.
    /// 创建圆形（全方向）视野源。
    ///
    /// This is the most common vision type, providing 360-degree visibility
    /// around the entity. Ideal for most units, NPCs, and light sources.
    ///
    /// # Parameters
    /// - `range`: Vision radius in world units
    ///
    /// # Performance
    /// - **Complexity**: O((R/S)²) where R=range, S=chunk_size
    /// - **GPU Cost**: Moderate - circle intersection tests
    ///
    /// # Example
    /// ```rust
    /// # use bevy_fog_of_war::prelude::*;
    /// let unit_vision = VisionSource::circle(120.0);
    /// let torch_vision = VisionSource::circle(80.0);
    /// ```
    pub fn circle(range: f32) -> Self {
        Self {
            range,
            enabled: true,
            shape: VisionShape::Circle,
            direction: 0.0,
            angle: std::f32::consts::FRAC_PI_2,
            intensity: 1.0,
            transition_ratio: 0.2,
        }
    }

    /// Creates a cone-shaped (directional) vision source.
    /// 创建扇形（方向性）视野源。
    ///
    /// Provides vision in a specific direction with a limited angle.
    /// Useful for units with facing direction, spotlights, or security cameras.
    ///
    /// # Parameters
    /// - `range`: Maximum vision distance from the entity
    /// - `direction`: Center direction in radians (0 = right, π/2 = up)
    /// - `angle`: Total cone angle in radians (not half-angle)
    ///
    /// # Performance
    /// - **Complexity**: O((R/S)² × A/2π) where A=angle
    /// - **GPU Cost**: Higher - requires angle calculations
    ///
    /// # Example
    /// ```rust
    /// # use bevy_fog_of_war::prelude::*;
    /// // 60-degree cone facing northeast
    /// let guard_vision = VisionSource::cone(
    ///     150.0,                        // 150 unit range
    ///     std::f32::consts::PI * 0.25,  // 45 degrees (northeast)
    ///     std::f32::consts::PI / 3.0    // 60 degree cone
    /// );
    /// ```
    pub fn cone(range: f32, direction: f32, angle: f32) -> Self {
        Self {
            range,
            enabled: true,
            shape: VisionShape::Cone,
            direction,
            angle,
            intensity: 1.0,
            transition_ratio: 0.2,
        }
    }

    /// Creates a square (rectangular) vision source.
    /// 创建正方形（矩形）视野源。
    ///
    /// Provides vision in a square area centered on the entity.
    /// Useful for buildings, watchtowers, or area-of-effect abilities.
    ///
    /// # Parameters
    /// - `range`: Half-width of the square (distance from center to edge)
    ///
    /// # Performance
    /// - **Complexity**: O((2R/S)²) where R=range, S=chunk_size
    /// - **GPU Cost**: Low - simple rectangle intersection tests
    ///
    /// # Example
    /// ```rust
    /// # use bevy_fog_of_war::prelude::*;
    /// // 200x200 unit square vision area
    /// let watchtower_vision = VisionSource::square(100.0);
    /// ```
    pub fn square(range: f32) -> Self {
        Self {
            range,
            enabled: true,
            shape: VisionShape::Square,
            direction: 0.0,
            angle: std::f32::consts::FRAC_PI_2,
            intensity: 1.0,
            transition_ratio: 0.2,
        }
    }
}

/// Geometric shape types for vision areas.
/// 视野形状
///
/// Defines the different geometric shapes that vision sources can use to determine
/// their area of effect. Each shape has different performance characteristics and
/// use cases in gameplay.
///
/// # Performance Comparison
/// - **Circle**: Moderate cost, most versatile
/// - **Square**: Lowest cost, simple calculations  
/// - **Cone**: Highest cost, complex angle math
///
/// # Shape Characteristics
/// Each shape interprets the `range` parameter differently:
/// - **Circle**: `range` = radius
/// - **Cone**: `range` = radius at maximum distance
/// - **Square**: `range` = half-width (center to edge)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Default)]
pub enum VisionShape {
    /// Circular vision providing 360-degree omnidirectional visibility.
    /// 圆形视野（全方向）
    ///
    /// The most common and versatile vision shape. Provides equal visibility
    /// in all directions from the entity's position.
    ///
    /// **Use Cases**:
    /// - Player units and NPCs
    /// - Light sources (torches, lamps)
    /// - Magical detection spells
    /// - Area-of-effect abilities
    ///
    /// **Performance**: O((R/S)²) where R=range, S=chunk_size
    #[default]
    Circle,

    /// Cone-shaped vision with directional focus and angular limits.
    /// 扇形视野（有方向和角度）
    ///
    /// Directional vision that only reveals areas within a specific angular range.
    /// Requires additional `direction` and `angle` parameters to define orientation.
    ///
    /// **Use Cases**:
    /// - Units with facing direction
    /// - Spotlights and flashlights  
    /// - Security cameras
    /// - Directional spells or abilities
    /// - Stealth gameplay mechanics
    ///
    /// **Performance**: O((R/S)² × A/2π) where A=angle in radians
    Cone,

    /// Square (rectangular) vision area centered on the entity.
    /// 正方形视野
    ///
    /// Axis-aligned rectangular vision area. Simple and efficient for
    /// buildings or large area coverage.
    ///
    /// **Use Cases**:
    /// - Buildings and structures
    /// - Watchtowers and guard posts
    /// - Large area-of-effect spells
    /// - Grid-based game mechanics
    ///
    /// **Performance**: O((2R/S)²) - very efficient, simple rectangle math
    Square,
}

impl Default for VisionSource {
    fn default() -> Self {
        Self {
            range: 100.0,
            enabled: true,
            shape: VisionShape::default(),
            direction: 0.0,
            angle: std::f32::consts::FRAC_PI_2, // 默认90度扇形 / Default 90 degree cone
            intensity: 1.0,
            transition_ratio: 0.2, // 默认20%的过渡区域 / Default 20% transition area
        }
    }
}

/// Visibility state enumeration for fog of war chunks.
/// 区块的可见性状态
///
/// Represents the three possible visibility states that a chunk can be in within the fog of war system.
/// This enum drives the visual representation and gameplay mechanics of explored vs unexplored areas.
///
/// # State Transitions
/// ```text
/// Unexplored -> Visible -> Explored -> Visible (repeatable)
/// ```
///
/// # Performance Characteristics
/// - **Memory**: 1 byte per chunk (Copy trait, no heap allocation)
/// - **Serialization**: Full serde support for persistence
/// - **Reflection**: Bevy reflection support for editor integration
///
/// # Persistence
/// Chunks maintain their `Explored` state permanently once discovered, enabling fog of war
/// mechanics where players can see previously explored areas but not current enemy activity.
///
/// # Usage in Systems
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_fog_of_war::prelude::*;
/// fn update_chunk_visibility(
///     mut chunks: Query<&mut FogChunk>,
///     vision_sources: Query<(&Transform, &VisionSource)>,
/// ) {
///     for mut chunk in chunks.iter_mut() {
///         let was_visible = chunk.state.visibility == ChunkVisibility::Visible;
///         // Update visibility based on vision sources...
///         if !was_visible && chunk.state.visibility == ChunkVisibility::Visible {
///             // Chunk newly visible - trigger exploration
///             chunk.state.visibility = ChunkVisibility::Explored;
///         }
///     }
/// }
/// ```
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Reflect, serde::Serialize, serde::Deserialize,
)]
#[reflect(Default)] // 允许通过反射获取默认值 / Allow getting default value via reflection
pub enum ChunkVisibility {
    /// Never been revealed by any vision source.
    /// 从未被任何视野源照亮过
    ///
    /// Initial state for all chunks. These areas appear completely fogged and provide
    /// no information to the player about terrain or entities within.
    ///
    /// **Gameplay**: Complete mystery - player has no knowledge of this area
    /// **Rendering**: Full fog overlay, typically dark or heavily obscured
    #[default]
    Unexplored,

    /// Was revealed before, but not currently in vision.
    /// 曾经被照亮过，但当前不在视野内
    ///
    /// Areas that were previously visible but are no longer within range of any vision sources.
    /// These areas show static snapshot information (buildings, terrain) but not dynamic entities.
    ///
    /// **Gameplay**: Player can see static elements but not current activity
    /// **Rendering**: Partial fog with cached snapshot data visible
    Explored,

    /// Currently being revealed by at least one vision source.
    /// 当前正被至少一个视野源照亮
    ///
    /// Areas actively within range of one or more vision sources. Shows real-time information
    /// including dynamic entities, current states, and live updates.
    ///
    /// **Gameplay**: Full visibility with real-time information
    /// **Rendering**: No fog, full color and detail
    Visible,
}

impl Display for ChunkVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkVisibility::Unexplored => write!(f, "Unexplored"),
            ChunkVisibility::Explored => write!(f, "Explored"),
            ChunkVisibility::Visible => write!(f, "Visible"),
        }
    }
}

/// Spatial fog chunk component representing a discrete region of the game world.
/// 地图区块组件，代表一个空间区域的迷雾和可见性数据
///
/// The core component of the chunk-based fog of war system. Each `FogChunk` represents
/// a fixed-size rectangular region of the game world with its own fog and exploration state.
/// The world is automatically divided into a grid of these chunks for efficient processing.
///
/// # Chunk System Architecture
///
/// The fog of war system divides the infinite game world into fixed-size chunks (typically 256x256 units).
/// Each chunk manages its own:
/// - **Visibility State**: Whether the chunk is unexplored, explored, or currently visible
/// - **GPU Textures**: Fog texture (current visibility) and snapshot texture (explored areas)
/// - **Memory Management**: Whether chunk data resides in CPU or GPU memory
///
/// # Performance Characteristics
/// - **Memory**: ~64 bytes per active chunk (small, cache-friendly)
/// - **Spatial Queries**: O(1) world position to chunk lookup
/// - **GPU Transfer**: Dynamic loading/unloading between CPU and GPU memory
/// - **Scalability**: Supports virtually unlimited world sizes
///
/// # Coordinate System
/// - Chunk coordinates are in chunk-space (e.g., (0,0), (1,0), (0,1))
/// - World bounds are in world-space units (e.g., pixels, meters)
/// - Coordinate conversion handled automatically by `FogMapSettings`
///
/// # Memory Management
/// Chunks can exist in different memory locations based on visibility and distance from camera:
/// - **GPU**: Active chunks being rendered
/// - **CPU**: Inactive chunks stored for later use
/// - **Pending**: Chunks being transferred between CPU and GPU
///
/// # Example Usage
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_fog_of_war::prelude::*;
/// // Query visible chunks within camera view
/// fn process_visible_chunks(
///     chunks: Query<&FogChunk>,
///     camera: Query<&Transform, With<FogOfWarCamera>>,
/// ) {
///     if let Ok(camera_transform) = camera.single() {
///         for chunk in chunks.iter() {
///             if chunk.state.visibility == ChunkVisibility::Visible {
///                 // Process currently visible chunk
///                 println!("Visible chunk at {:?}", chunk.coords);
///             }
///         }
///     }
/// }
/// ```
#[derive(Component, ExtractComponent, Reflect, Debug, Clone)]
pub struct FogChunk {
    /// Chunk coordinates in chunk-space (not world coordinates).
    /// 区块坐标
    ///
    /// Grid coordinates identifying this chunk's position in the chunk grid.
    /// For example, chunk (0,0) covers world area \[0,0\] to \[chunk_size, chunk_size\].
    ///
    /// **Range**: Theoretically unlimited (i32 bounds)
    /// **Uniqueness**: Each coordinate pair represents exactly one chunk
    pub coords: IVec2,

    /// Layer index for this chunk in the fog TextureArray.
    /// 此区块在雾效 TextureArray 中的层索引
    ///
    /// When allocated on GPU, this specifies which layer of the fog texture array
    /// contains this chunk's current visibility data. `None` when chunk is CPU-only.
    ///
    /// **GPU Texture Arrays**: Enable efficient batch rendering of multiple chunks
    /// **Dynamic Allocation**: Indices assigned when chunk becomes visible
    pub fog_layer_index: Option<u32>,

    /// Layer index for this chunk in the snapshot TextureArray.
    /// 此区块在快照 TextureArray 中的层索引
    ///
    /// When allocated on GPU, this specifies which layer of the snapshot texture array
    /// contains this chunk's exploration snapshot data. `None` when chunk is CPU-only.
    ///
    /// **Snapshot Data**: Preserves explored areas for persistent fog of war
    /// **Memory Efficiency**: Only allocated for explored chunks
    pub snapshot_layer_index: Option<u32>,

    /// Current aggregated state of the chunk (visibility and memory location).
    /// 区块的当前状态 (可见性与内存位置)
    ///
    /// Combines visibility state (unexplored/explored/visible) with memory location
    /// (CPU/GPU/pending) for efficient state management and rendering decisions.
    ///
    /// **State Synchronization**: Updated by multiple systems working together
    /// **Performance**: Single field access for common state queries
    pub state: ChunkState,

    /// World space boundaries of the chunk in world units.
    /// 区块的世界空间边界（以像素/单位为单位）
    ///
    /// Defines the rectangular area in world coordinates that this chunk covers.
    /// Used for spatial queries, collision detection, and viewport culling.
    ///
    /// **Coordinate System**: World units (same as Transform positions)
    /// **Immutable**: Set once during chunk creation based on chunk_size settings
    pub world_bounds: Rect,
}

impl FogChunk {
    /// Generates a unique identifier for this chunk based on its coordinates.
    /// 根据坐标生成区块的唯一标识符
    ///
    /// Creates a deterministic 32-bit hash from the chunk coordinates by packing
    /// both X and Y coordinates into a single integer. This is used for efficient
    /// chunk identification in hash maps and GPU data structures.
    ///
    /// # Algorithm
    /// - Shifts coordinates by +32768 to handle negative coordinates
    /// - Packs X into upper 16 bits, Y into lower 16 bits
    /// - Results in bijective mapping from coordinates to IDs
    ///
    /// # Performance
    /// - **Time Complexity**: O(1) - simple bit operations
    /// - **Memory**: No allocation, returns primitive type
    /// - **Deterministic**: Same coordinates always produce same ID
    ///
    /// # Coordinate Range
    /// Supports chunk coordinates from -32768 to +32767 in both axes.
    ///
    /// # Example
    /// ```rust
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let chunk = FogChunk::new(IVec2::new(10, 20), UVec2::new(256, 256), 1.0);
    /// let id = chunk.unique_id();
    /// // Same coordinates will always produce the same ID
    /// assert_eq!(id, chunk.unique_id());
    /// ```
    pub fn unique_id(&self) -> u32 {
        // Offset coordinates to handle negative values (range: -32768 to +32767)
        let ox = (self.coords.x + 32768) as u32;
        let oy = (self.coords.y + 32768) as u32;
        // Pack coordinates: X in upper 16 bits, Y in lower 16 bits
        (ox << 16) | (oy & 0xFFFF)
    }

    /// Creates a new fog chunk with calculated world boundaries.
    /// 创建一个新的地图区块
    ///
    /// Factory method for creating a new chunk with properly calculated world space boundaries.
    /// The chunk starts in an unexplored state with no GPU texture allocations.
    ///
    /// # Parameters
    /// - `chunk_coord`: Grid coordinates of the chunk (e.g., (0,0), (1,0))
    /// - `size`: Size of the chunk in grid units (typically UVec2(256, 256))
    /// - `tile_size`: Size of each grid unit in world coordinates
    ///
    /// # World Bounds Calculation
    /// ```text
    /// world_min = chunk_coord * size * tile_size
    /// world_max = world_min + (size * tile_size)
    /// ```
    ///
    /// # Performance
    /// - **Time Complexity**: O(1) - simple arithmetic operations
    /// - **Memory**: Allocates single chunk instance (~64 bytes)
    /// - **GPU**: No GPU resources allocated initially
    ///
    /// # Example
    /// ```rust
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// // Create chunk (1,1) with 256x256 grid cells, each 1.0 world unit
    /// let chunk = FogChunk::new(
    ///     IVec2::new(1, 1),    // Chunk coordinates
    ///     UVec2::new(256, 256), // Chunk size in grid cells
    ///     1.0                   // Each cell is 1.0 world units
    /// );
    ///
    /// // This chunk covers world area [256, 256] to [512, 512]
    /// assert_eq!(chunk.world_bounds.min, Vec2::new(256.0, 256.0));
    /// assert_eq!(chunk.world_bounds.max, Vec2::new(512.0, 512.0));
    /// ```
    pub fn new(chunk_coord: IVec2, size: UVec2, tile_size: f32) -> Self {
        // Calculate world-space boundaries from chunk coordinates
        let min = Vec2::new(
            chunk_coord.x as f32 * size.x as f32 * tile_size,
            chunk_coord.y as f32 * size.y as f32 * tile_size,
        );
        let max = min + Vec2::new(size.x as f32 * tile_size, size.y as f32 * tile_size);

        Self {
            coords: chunk_coord,
            fog_layer_index: None,      // No GPU allocation initially
            snapshot_layer_index: None, // No GPU allocation initially
            state: Default::default(),  // Unexplored, CPU memory
            world_bounds: Rect { min, max },
        }
    }

    /// Tests whether a world position falls within this chunk's boundaries.
    /// 判断一个世界坐标是否在该区块内
    ///
    /// Performs an axis-aligned bounding box (AABB) containment test to determine
    /// if the given world position is within this chunk's coverage area.
    ///
    /// # Parameters
    /// - `world_pos`: Position in world coordinates to test
    ///
    /// # Returns
    /// `true` if the position is within the chunk bounds (inclusive), `false` otherwise.
    ///
    /// # Performance
    /// - **Time Complexity**: O(1) - simple coordinate comparison
    /// - **Memory**: No allocation, operates on stack values
    /// - **Branch Prediction**: Highly optimized by CPU for repeated calls
    ///
    /// # Boundary Behavior
    /// Uses inclusive bounds checking:
    /// - `min` coordinates are inclusive (position exactly on min edge counts as inside)
    /// - `max` coordinates are exclusive (position exactly on max edge counts as outside)
    ///
    /// # Example
    /// ```rust
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let chunk = FogChunk::new(IVec2::ZERO, UVec2::new(256, 256), 1.0);
    ///
    /// // Test positions
    /// assert!(chunk.contains_world_pos(Vec2::new(100.0, 100.0))); // Inside
    /// assert!(chunk.contains_world_pos(Vec2::new(0.0, 0.0)));     // On min edge (inclusive)
    /// assert!(chunk.contains_world_pos(Vec2::new(255.9, 255.9))); // Near max edge (inside)
    /// assert!(!chunk.contains_world_pos(Vec2::new(-10.0, 100.0))); // Outside
    /// ```
    pub fn contains_world_pos(&self, world_pos: Vec2) -> bool {
        self.world_bounds.contains(world_pos)
    }
}

/// GPU texture handle component for fog chunk rendering.
/// 雾效区块纹理句柄组件
///
/// Manages the Bevy image asset handles for a chunk's GPU textures. Each chunk
/// that exists on the GPU has two associated textures: a fog texture for current
/// visibility and a snapshot texture for persistent exploration data.
///
/// # Texture Types
/// - **Fog Texture**: Real-time visibility data updated by compute shaders
/// - **Snapshot Texture**: Persistent exploration data saved when areas become explored
///
/// # Memory Management
/// The actual texture data is managed by Bevy's asset system. This component only
/// holds handles (references) to the textures, enabling efficient sharing and
/// automatic cleanup when chunks are despawned.
///
/// # Performance Characteristics
/// - **Memory**: ~16 bytes per component (just handles, not texture data)
/// - **Texture Assets**: Managed by Bevy's asset system with reference counting
/// - **GPU Memory**: Actual texture data allocated on GPU via render world
///
/// # Lifecycle
/// 1. Created when chunk allocated on GPU via `from_setting()`
/// 2. Handles passed to render world for GPU access
/// 3. Automatically cleaned up when component is despawned
///
/// # Example Usage
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_fog_of_war::prelude::*;
/// fn create_chunk_with_textures(
///     mut commands: Commands,
///     mut images: ResMut<Assets<Image>>,
///     settings: Res<FogMapSettings>,
/// ) {
///     let chunk_image = FogChunkImage::from_setting(&mut images, &settings);
///     commands.spawn((
///         FogChunk::new(IVec2::ZERO, settings.chunk_size, 1.0),
///         chunk_image,
///     ));
/// }
/// ```
#[derive(Component, Reflect, Debug, Clone)]
pub struct FogChunkImage {
    /// Handle to the fog texture showing current visibility.
    /// 雾效纹理句柄，显示当前可见性
    ///
    /// This texture is continuously updated by GPU compute shaders to reflect
    /// real-time visibility changes as vision sources move and are enabled/disabled.
    ///
    /// **Format**: Typically R8 (8-bit grayscale) for memory efficiency
    /// **Usage**: Updated by compute shaders, read by fragment shaders
    pub fog_image_handle: Handle<Image>,

    /// Handle to the snapshot texture preserving exploration history.
    /// 快照纹理句柄，保存探索历史
    ///
    /// This texture preserves areas that have been explored, creating the persistent
    /// fog of war effect where players can see terrain they've discovered but not
    /// current enemy activity.
    ///
    /// **Format**: Typically R8 (8-bit grayscale) matching fog texture
    /// **Usage**: Updated when areas become explored, read for rendering explored regions
    pub snapshot_image_handle: Handle<Image>,
}

impl FogChunkImage {
    /// Creates chunk textures from fog map settings (ResMut wrapper).
    /// 从雾效地图设置创建区块纹理（ResMut包装器）
    ///
    /// Convenience wrapper around `from_setting_raw()` that accepts a `ResMut<Assets<Image>>`
    /// parameter. This is the most common way to create chunk textures from systems.
    ///
    /// # Parameters
    /// - `images`: Mutable reference to the image asset storage
    /// - `setting`: Fog map configuration determining texture properties
    ///
    /// # Performance
    /// - **Time Complexity**: O(1) - delegates to `from_setting_raw()`
    /// - **Memory**: Allocates two textures based on settings configuration
    ///
    /// # Example
    /// ```rust
    /// # use bevy::prelude::*;
    /// # use bevy_fog_of_war::prelude::*;
    /// fn system(
    ///     mut images: ResMut<Assets<Image>>,
    ///     settings: Res<FogMapSettings>,
    /// ) {
    ///     let chunk_image = FogChunkImage::from_setting(&mut images, &settings);
    /// }
    /// ```
    pub fn from_setting(images: &mut ResMut<Assets<Image>>, setting: &FogMapSettings) -> Self {
        Self::from_setting_raw(images, setting)
    }

    /// Creates fog and snapshot textures based on configuration settings.
    /// 根据配置设置创建雾效和快照纹理
    ///
    /// Factory method that creates properly configured GPU textures for a fog chunk.
    /// Both textures are created with identical dimensions but may use different formats
    /// based on the settings configuration.
    ///
    /// # Texture Configuration
    /// - **Resolution**: Determined by `texture_resolution_per_chunk` setting
    /// - **Format**: Uses `fog_texture_format` and `snapshot_texture_format` from settings
    /// - **Usage**: Configured for bidirectional CPU↔GPU transfer
    /// - **Sampling**: Linear interpolation for smooth fog transitions
    ///
    /// # Memory Layout
    /// Both textures are initialized with zero-filled data and configured for:
    /// - `COPY_DST`: Enable CPU→GPU transfers for loading data
    /// - `COPY_SRC`: Enable GPU→CPU transfers for persistence
    ///
    /// # Parameters
    /// - `images`: Direct access to image asset storage
    /// - `setting`: Configuration containing texture format and resolution
    ///
    /// # Performance
    /// - **Time Complexity**: O(W×H) where W,H are texture dimensions
    /// - **Memory**: Allocates 2 textures × (width × height × bytes_per_pixel)
    /// - **GPU**: Textures allocated immediately and uploaded to GPU
    ///
    /// # Texture Formats
    /// Common configurations:
    /// - **R8**: 1 byte per pixel for grayscale fog (most memory efficient)
    /// - **RG8**: 2 bytes per pixel for fog + additional data
    /// - **RGBA8**: 4 bytes per pixel for full color information
    ///
    /// # Example
    /// ```rust
    /// # use bevy::prelude::*;
    /// # use bevy_fog_of_war::prelude::*;
    /// let mut images = Assets::<Image>::default();
    /// let settings = FogMapSettings::default();
    /// let chunk_image = FogChunkImage::from_setting_raw(&mut images, &settings);
    /// ```
    pub fn from_setting_raw(images: &mut Assets<Image>, setting: &FogMapSettings) -> Self {
        // Create fog texture with zero-filled initial data
        let data = vec![0u8; setting.fog_texture_format.pixel_size()];
        let mut fog_image = Image::new_fill(
            Extent3d {
                width: setting.texture_resolution_per_chunk.x,
                height: setting.texture_resolution_per_chunk.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &data,
            setting.fog_texture_format,
            RenderAssetUsages::default(),
        );
        // Configure texture for bidirectional CPU↔GPU transfers
        fog_image.texture_descriptor.usage = TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
            | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输
        // Enable linear sampling for smooth fog transitions
        fog_image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
        let fog_image_handle = images.add(fog_image);

        // Create snapshot texture with identical configuration
        let data = vec![0u8; setting.snapshot_texture_format.pixel_size()];
        let mut snapshot_image = Image::new_fill(
            Extent3d {
                width: setting.texture_resolution_per_chunk.x,
                height: setting.texture_resolution_per_chunk.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &data,
            setting.snapshot_texture_format,
            RenderAssetUsages::default(),
        );
        // Configure snapshot texture for bidirectional CPU↔GPU transfers
        snapshot_image.texture_descriptor.usage = TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
            | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输
        // Enable linear sampling for smooth exploration transitions
        snapshot_image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
        let snapshot_image_handle = images.add(snapshot_image);

        Self {
            fog_image_handle,
            snapshot_image_handle,
        }
    }
}

/// Memory location enumeration for chunk texture data management.
/// 区块纹理数据的存储位置
///
/// Tracks where a chunk's texture data currently resides to enable efficient CPU↔GPU
/// memory management. The fog of war system dynamically moves chunk data between CPU
/// and GPU memory based on visibility and distance from camera.
///
/// # Memory Management Strategy
/// - **GPU**: Active chunks currently visible or near camera
/// - **CPU**: Distant chunks to free GPU memory for active areas
/// - **Pending**: Transition states during asynchronous transfers
///
/// # State Transitions
/// ```text
/// Cpu ←→ PendingCopyToGpu ←→ Gpu ←→ PendingCopyToCpu ←→ Cpu
/// ```
///
/// # Performance Impact
/// - **GPU Memory**: Limited resource, carefully managed
/// - **Transfer Cost**: CPU↔GPU transfers are expensive but amortized
/// - **Rendering**: Only GPU-resident chunks can be rendered
///
/// # Asynchronous Operations
/// Transfer operations are asynchronous to avoid blocking the main thread:
/// - Transfers requested by main world
/// - Executed by render world
/// - Completion signaled via events
///
/// # Example Usage
/// ```rust
/// # use bevy_fog_of_war::prelude::*;
/// fn manage_chunk_memory(chunk: &mut FogChunk, distance_to_camera: f32) {
///     match (chunk.state.memory_location, distance_to_camera) {
///         (ChunkMemoryLocation::Gpu, dist) if dist > 1000.0 => {
///             // Far from camera - consider moving to CPU
///             chunk.state.memory_location = ChunkMemoryLocation::PendingCopyToCpu;
///         },
///         (ChunkMemoryLocation::Cpu, dist) if dist < 500.0 => {
///             // Near camera - consider moving to GPU
///             chunk.state.memory_location = ChunkMemoryLocation::PendingCopyToGpu;
///         },
///         _ => {} // No change needed
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Default)]
pub enum ChunkMemoryLocation {
    /// Texture data resides in GPU VRAM, ready for rendering.
    /// 纹理数据存储在 GPU 显存中，可用于渲染
    ///
    /// The chunk's fog and snapshot textures are allocated on the GPU and available
    /// for immediate rendering. This is the preferred state for visible chunks.
    ///
    /// **Performance**: Optimal rendering performance, no transfer latency
    /// **Memory Cost**: Consumes limited GPU VRAM
    /// **Rendering**: Can be rendered immediately
    #[default]
    Gpu,

    /// Texture data is unloaded from GPU and stored in CPU RAM.
    /// 纹理数据已从 GPU 卸载，存储在 CPU 内存中
    ///
    /// The chunk's texture data has been moved to system RAM to free GPU memory.
    /// Chunks in this state cannot be rendered until moved back to GPU.
    ///
    /// **Performance**: No rendering capability, requires transfer to render
    /// **Memory Cost**: Uses abundant system RAM instead of limited GPU VRAM
    /// **Use Case**: Distant chunks unlikely to be visible soon
    Cpu,

    /// Main world has requested render world to copy this chunk's data from GPU.
    /// 主世界已请求渲染世界将此区块数据从 GPU 复制到 CPU
    ///
    /// Transitional state indicating a GPU→CPU transfer is in progress.
    /// The chunk remains renderable until the transfer completes.
    ///
    /// **Trigger**: Chunk moved far from camera or GPU memory pressure
    /// **Completion**: ChunkGpuDataReady event signals transfer completion
    /// **Next State**: Transitions to `Cpu` when transfer completes
    PendingCopyToCpu,

    /// Main world has requested render world to upload CPU data to GPU texture.
    /// 主世界已请求渲染世界将 CPU 数据上传到此区块的 GPU 纹理
    ///
    /// Transitional state indicating a CPU→GPU transfer is in progress.
    /// The chunk is not yet renderable until the transfer completes.
    ///
    /// **Trigger**: Chunk moved near camera or became visible
    /// **Completion**: ChunkCpuDataUploaded event signals transfer completion
    /// **Next State**: Transitions to `Gpu` when transfer completes
    PendingCopyToGpu,
}

/// Aggregated state container for fog chunks.
/// 聚合区块状态
///
/// Combines visibility state and memory location into a single convenient struct
/// for efficient state management and queries. This reduces the need to access
/// multiple separate fields when making chunk management decisions.
///
/// # Design Rationale
/// Rather than storing visibility and memory location separately, this struct
/// provides a unified state representation that simplifies system logic and
/// reduces the cognitive overhead of chunk state management.
///
/// # Performance Characteristics
/// - **Memory**: 2 bytes total (1 byte per enum)
/// - **Copy Semantics**: Cheap to copy and pass by value
/// - **Cache Friendly**: Small size promotes cache efficiency
/// - **Query Efficiency**: Single component for common state queries
///
/// # Common State Combinations
/// - `Unexplored + Gpu`: New chunk allocated but not yet revealed
/// - `Visible + Gpu`: Active chunk currently being rendered
/// - `Explored + Cpu`: Previously explored chunk stored in system memory
/// - `Explored + PendingCopyToGpu`: Chunk being loaded back for rendering
///
/// # State Synchronization
/// Both fields are updated by different systems but should remain consistent:
/// - Visibility updated by fog calculation systems
/// - Memory location updated by memory management systems
/// - Combined state used by rendering and transfer systems
///
/// # Example Usage
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_fog_of_war::prelude::*;
/// fn process_chunk_states(chunks: Query<&FogChunk>) {
///     for chunk in chunks.iter() {
///         match chunk.state {
///             ChunkState {
///                 visibility: ChunkVisibility::Visible,
///                 memory_location: ChunkMemoryLocation::Gpu
///             } => {
///                 // Chunk is visible and ready to render
///                 println!("Rendering chunk {:?}", chunk.coords);
///             },
///             ChunkState {
///                 visibility: ChunkVisibility::Explored,
///                 memory_location: ChunkMemoryLocation::Cpu
///             } => {
///                 // Chunk was explored but stored in CPU memory
///                 println!("Explored chunk {:?} in CPU memory", chunk.coords);
///             },
///             _ => {} // Handle other state combinations
///         }
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect, Default)]
#[reflect(Default)] // 允许通过反射获取默认值 / Allow getting default value via reflection
pub struct ChunkState {
    /// Current visibility state of the chunk.
    /// 可见性状态
    ///
    /// Determines whether the chunk is unexplored, previously explored but not
    /// currently visible, or actively visible. This affects how the chunk is
    /// rendered and what information is displayed to the player.
    ///
    /// **Updated by**: Vision calculation and fog update systems
    /// **Used by**: Rendering, UI, and gameplay systems
    pub visibility: ChunkVisibility,

    /// Current memory storage location of the chunk's texture data.
    /// 内存存储位置
    ///
    /// Tracks whether the chunk's texture data is currently on the GPU (ready for
    /// rendering), stored in CPU memory (not renderable), or in a transitional
    /// state during transfer operations.
    ///
    /// **Updated by**: Memory management and transfer systems
    /// **Used by**: Rendering, memory management, and persistence systems
    pub memory_location: ChunkMemoryLocation,
}
