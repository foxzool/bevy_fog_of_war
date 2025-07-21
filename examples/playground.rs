//! Interactive Fog of War Playground Example
//! 交互式战争迷雾游乐场示例
//!
//! This comprehensive example demonstrates the full capabilities of the bevy_fog_of_war
//! plugin through an interactive playground with multiple vision sources, camera controls,
//! persistence systems, and real-time debugging features.
//!
//! # Features Demonstrated
//!
//! ## Core Fog of War Functionality
//! - **Multiple Vision Sources**: Static and dynamic vision providers with different shapes
//! - **Camera-Based Exploration**: Primary camera with FogOfWarCamera component
//! - **Real-time Updates**: Fog responds instantly to entity movement
//! - **Configurable Settings**: Runtime adjustment of fog properties
//!
//! ## Interactive Controls
//! - **WASD**: Camera movement for exploring the world
//! - **Arrow Keys**: Direct control of player vision source
//! - **Mouse Controls**: Click-to-move targeting for player entity
//! - **F Key**: Toggle fog rendering on/off
//! - **R Key**: Complete fog reset to clear all exploration
//! - **PageUp/PageDown**: Adjust fog transparency
//!
//! ## Persistence System
//! - **P Key**: Save fog data with automatic format selection (bincode > messagepack > json)
//! - **L Key**: Load fog data with automatic format detection
//! - **F12 Key**: Force snapshot all Capturable entities currently on screen
//! - **Compression Support**: Automatic zstd/lz4 compression when available
//! - **Format Fallback**: Intelligent fallback to available serialization formats
//!
//! ## Entity Types and Behaviors
//!
//! ### Vision Sources
//! - **Player**: Controllable circle vision (blue, 100 range)
//! - **Static Source**: Fixed square vision (gold, 40 range)
//! - **Geometric Shapes**: Cone vision with rotating directions (various ranges)
//!
//! ### Capturable Entities
//! - **Rotating Entities**: Visible in snapshots when first explored
//! - **Moving Sprite**: Horizontal movement with boundary collision
//! - **Static Shapes**: Various geometric forms for visual testing
//!
//! ## Performance Monitoring
//! - **FPS Display**: Real-time frame rate monitoring
//! - **Chunk Statistics**: Active chunk count and camera view metrics
//! - **Debug Visualization**: Chunk boundaries and layer indices when fog disabled
//! - **Event Logging**: Comprehensive logging of fog operations
//!
//! # Usage Examples
//!
//! ## Basic Exploration
//! ```bash
//! cargo run --example playground
//! # Use WASD to move camera around the world
//! # Use arrow keys to move the blue player entity
//! # Watch fog clear as you explore new areas
//! ```
//!
//! ## Persistence Testing
//! ```bash
//! # Explore some areas, then save
//! # Press P to save fog data
//! # Press R to reset fog
//! # Press L to load saved data
//! ```
//!
//! ## Performance Analysis
//! ```bash
//! # Disable fog to see chunk debug info
//! # Press F to toggle fog rendering
//! # Observe chunk boundaries and statistics
//! # Monitor FPS in top-left corner
//! ```
//!
//! # Architecture
//!
//! ## System Organization
//! The example follows a modular system architecture:
//! - **Setup Systems**: `setup`, `setup_ui` - Initialize world and UI
//! - **Input Systems**: Camera movement, vision control, keyboard shortcuts
//! - **Update Systems**: FPS display, fog settings, entity animation
//! - **Debug Systems**: Chunk visualization, event logging
//! - **Persistence Systems**: Save/load handling with format detection
//!
//! ## Entity Spawning Pattern
//! ```rust
//! // Camera with fog support
//! commands.spawn((Camera2d, FogOfWarCamera));
//!
//! // Vision source entity
//! commands.spawn((
//!     Sprite { /* ... */ },
//!     Transform::from_translation(position),
//!     VisionSource {
//!         range: 100.0,
//!         shape: VisionShape::Circle,
//!         // ... other properties
//!     },
//! ));
//!
//! // Capturable entity (appears in snapshots)
//! commands.spawn((
//!     Sprite { /* ... */ },
//!     Transform::from_translation(position),
//!     Capturable,
//! ));
//! ```
//!
//! ## Performance Characteristics
//! - **Entity Count**: ~15 entities total (minimal overhead)
//! - **Vision Sources**: 6 active vision sources with different shapes
//! - **Update Frequency**: 60 FPS target with real-time fog updates
//! - **Memory Usage**: Dynamic chunk loading based on camera view
//! - **GPU Usage**: Efficient compute shaders for fog calculations
//!
//! # Educational Value
//!
//! This example serves as:
//! - **Integration Guide**: Shows proper plugin setup and configuration
//! - **Feature Showcase**: Demonstrates all major fog of war capabilities
//! - **Performance Baseline**: Provides reference implementation for optimization
//! - **Testing Environment**: Interactive sandbox for experimenting with settings
//! - **Debug Tool**: Visual debugging of chunk systems and fog rendering
//!
//! # File Structure
//! - **Components**: Custom marker components for different entity types
//! - **Resources**: Global state management (TargetPosition)
//! - **Systems**: Modular functions handling specific behaviors
//! - **Setup Functions**: World initialization and UI creation
//! - **Event Handlers**: Persistence and reset event processing

use bevy::diagnostic::FrameCount;
use bevy::{
    color::palettes::css::{GOLD, RED},
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_fog_of_war::prelude::*;

/// Global resource tracking the target position for player movement via mouse clicks.
/// 全局资源，跟踪通过鼠标点击进行玩家移动的目标位置
///
/// This resource enables click-to-move functionality for the player entity.
/// When the user clicks somewhere in the world, the world coordinates are
/// stored here and the movable vision control system smoothly moves the
/// player toward that target.
///
/// # State Management
/// - **Some(Vec3)**: Active target position, player moving toward it
/// - **None**: No active target, player stationary or under manual control
///
/// # Integration Points
/// - **movable_vision_control**: Reads target and moves player entity
/// - **Mouse Input**: Sets target when left mouse button clicked
/// - **Keyboard Input**: Clears target when arrow keys pressed (manual control)
///
/// # Performance Characteristics
/// - **Memory**: Minimal - single Optional Vec3
/// - **Update Frequency**: Only when mouse clicked or target reached
/// - **Time Complexity**: O(1) access and modification
#[derive(Resource, Default)]
struct TargetPosition(Option<Vec3>);

/// Marker component identifying the main player entity in the fog of war example.
/// 标识战争迷雾示例中主要玩家实体的标记组件
///
/// This component marks the entity that represents the player character,
/// which typically has controllable vision and movement capabilities.
/// Used for queries that need to specifically target the player entity.
///
/// # Usage Pattern
/// - **Player Entity**: Spawned with MovableVision, VisionSource, and Player components
/// - **System Queries**: Used to filter player-specific logic
/// - **Visual Distinction**: Player entity has unique color and behavior
///
/// # Integration
/// - **Movement Systems**: Arrow key and mouse control target this entity
/// - **Vision System**: Player provides primary exploration capability
/// - **Event Handlers**: Player-specific persistence or reset logic
#[derive(Component)]
struct Player;

/// Main entry point for the fog of war playground example.
/// 战争迷雾游乐场示例的主入口点
///
/// Sets up a complete Bevy application with:
/// 1. **Window Configuration**: 1280x720 resolution with custom title
/// 2. **Rendering Setup**: Nearest neighbor filtering for pixel-perfect sprites
/// 3. **Diagnostics**: Frame time monitoring for performance analysis
/// 4. **Fog of War**: Complete plugin with all features enabled
/// 5. **Systems**: All interactive, UI, debug, and persistence systems
///
/// # Plugin Configuration
/// - **DefaultPlugins**: Standard Bevy functionality with custom settings
/// - **FrameTimeDiagnosticsPlugin**: FPS monitoring and display
/// - **FogOfWarPlugin**: Core fog of war functionality
///
/// # System Scheduling
/// - **Startup**: `setup`, `setup_ui` - Initialize world content and UI
/// - **Update**: All interactive and monitoring systems run each frame
///
/// # Performance Expectations
/// - **Target FPS**: 60 FPS with smooth fog updates
/// - **Entity Count**: ~15 entities total
/// - **Memory Usage**: Dynamic based on explored area
/// - **GPU Load**: Moderate for compute shader fog calculations
///
/// # Error Handling
/// The application will panic if essential resources (fonts, etc.) cannot be loaded.
fn main() {
    App::new()
        .insert_resource(ClearColor(Color::WHITE))
        .insert_resource(TargetPosition(None))
        .add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Fog of War Example".into(),
                        resolution: (1280.0, 720.0).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
            FrameTimeDiagnosticsPlugin::default(),
            // LogDiagnosticsPlugin::default(),
            // bevy_render::diagnostic::RenderDiagnosticsPlugin,
        ))
        .init_gizmo_group::<MyRoundGizmos>()
        // .add_plugins(bevy_inspector_egui::bevy_egui::EguiPlugin {
        //     enable_multipass_for_primary_context: true,
        // })
        // .add_plugins(bevy_inspector_egui::quick::WorldInspectorPlugin::new())
        .add_plugins(FogOfWarPlugin)
        .add_systems(Startup, (setup, setup_ui))
        .add_systems(
            Update,
            (
                camera_movement,
                update_count_text,
                update_fog_settings,
                update_fps_text,
                movable_vision_control,
                debug_draw_chunks,
                horizontal_movement_system,
                handle_fog_reset_events,
                rotate_entities_system,
                handle_reset_input,
                handle_persistence_input,
                handle_saved_event,
                handle_loaded_event,
            ),
        )
        .run();
}

/// Gizmo configuration group for custom debug drawing in the fog of war example.
/// 战争迷雾示例中自定义调试绘制的Gizmo配置组
///
/// This configuration group enables custom gizmo drawing for debug visualization,
/// particularly useful for drawing chunk boundaries and fog-related debug information.
///
/// # Usage
/// - **Debug Drawing**: Used in `debug_draw_chunks` system
/// - **Chunk Visualization**: Draws chunk boundaries when fog is disabled
/// - **Performance**: Gizmos are only drawn when needed for debugging
///
/// # Integration with Bevy
/// Registered via `init_gizmo_group::<MyRoundGizmos>()` in main function.
#[derive(Default, Reflect, GizmoConfigGroup)]
struct MyRoundGizmos {}

/// Marker component for entities that should have fog material applied.
/// 应该应用雾效材质的实体的标记组件
///
/// This component marks camera entities that need fog material management.
/// Used by fog settings systems to identify which cameras should have
/// their fog rendering properties updated.
///
/// # Usage Pattern
/// - **Camera Marking**: Applied to cameras that render fog effects
/// - **Material Management**: Enables dynamic fog material addition/removal
/// - **Settings Integration**: Used by `update_fog_settings` for fog toggling
///
/// # System Integration
/// The `update_fog_settings` system queries for entities with this component
/// to apply or remove fog materials based on user input and settings.
#[derive(Component)]
struct FogMaterialComponent;

/// Marker component for UI text elements that display frame rate information.
/// 显示帧率信息的UI文本元素的标记组件
///
/// This component identifies text spans that should be updated with current
/// FPS values. Used by the `update_fps_text` system to target specific
/// UI elements for real-time performance monitoring.
///
/// # UI Integration
/// - **Text Location**: Typically positioned in top-left corner
/// - **Update Frequency**: Updated every frame with smoothed FPS values
/// - **Format**: Displays FPS with one decimal place precision
///
/// # Performance Monitoring
/// Provides real-time feedback on application performance, useful for:
/// - **Optimization**: Identifying performance bottlenecks
/// - **Debugging**: Monitoring impact of fog operations
/// - **User Feedback**: Visual indication of application responsiveness
#[derive(Component)]
struct FpsText;

/// Marker component for UI text elements that display fog configuration and statistics.
/// 显示雾效配置和统计信息的UI文本元素的标记组件
///
/// This component identifies text elements that should be updated with current
/// fog settings, including enabled state, transparency levels, and chunk statistics.
/// Multiple systems update this text with different types of information.
///
/// # Content Types
/// - **Fog Status**: Enabled/Disabled state
/// - **Alpha Levels**: Current transparency percentage
/// - **Control Instructions**: Key bindings for fog manipulation
/// - **Chunk Statistics**: Total chunks and chunks in camera view
///
/// # System Integration
/// - **update_fog_settings**: Updates fog status and alpha information
/// - **debug_draw_chunks**: Adds chunk count statistics
/// - **Real-time Updates**: Text refreshed every frame when relevant settings change
///
/// # Performance Impact
/// - **Update Cost**: Minimal string formatting overhead
/// - **Memory**: Small string allocations for text updates
/// - **Frequency**: Only updates when settings change or in debug mode
#[derive(Component)]
struct FogSettingsText;

/// Marker component for UI text elements that have animated color effects.
/// 具有动画颜色效果的UI文本元素的标记组件
///
/// This component identifies text elements that should have their colors
/// animated over time, typically used for visual flair in UI titles or
/// important information displays.
///
/// # Animation Properties
/// - **Color Cycling**: Text color changes over time
/// - **Visual Appeal**: Adds dynamic visual interest to static UI
/// - **Attention Drawing**: Helps highlight important information
///
/// # Usage Context
/// Currently applied to the "Fog of War" title text in the bottom-right
/// corner of the screen, though the animation system is not yet implemented
/// in this example (placeholder for future enhancement).
///
/// # Performance Considerations
/// - **CPU Cost**: Minimal color interpolation calculations
/// - **Update Frequency**: Would run every frame when implemented
/// - **Memory**: No additional memory overhead beyond component storage
#[derive(Component)]
struct ColorAnimatedText;

/// Marker component for UI text elements that display frame count information.
/// 显示帧计数信息的UI文本元素的标记组件
///
/// This component identifies text elements that should be updated with the
/// current frame count from Bevy's FrameCount resource. Provides a simple
/// way to track application runtime and frame progression.
///
/// # Display Properties
/// - **Format**: "Count: {frame_number}"
/// - **Update Frequency**: Every frame
/// - **Location**: Positioned in world space near other debug information
///
/// # Use Cases
/// - **Debug Information**: Track frame progression during testing
/// - **Performance Analysis**: Correlate events with specific frame numbers
/// - **Animation Timing**: Reference point for frame-based animations
///
/// # System Integration
/// Updated by the `update_count_text` system which reads from Bevy's
/// built-in FrameCount resource and formats the display text.
#[derive(Component)]
struct CountText;

/// Marker component for vision source entities that can be controlled by user input.
/// 可以由用户输入控制的视野源实体的标记组件
///
/// This component identifies entities that should respond to movement controls,
/// including both keyboard input (arrow keys) and mouse click-to-move targeting.
/// Typically applied to the player entity for interactive exploration.
///
/// # Control Methods
/// - **Arrow Keys**: Direct movement in cardinal directions
/// - **Mouse Clicks**: Click-to-move targeting with smooth pathfinding
/// - **Hybrid Control**: Arrow keys cancel mouse targets for immediate control
///
/// # Movement Characteristics
/// - **Speed**: 200.0 units per second
/// - **Smoothing**: Gradual acceleration/deceleration for click-to-move
/// - **Precision**: 5.0 unit tolerance for reaching click targets
///
/// # System Integration
/// - **movable_vision_control**: Primary movement system for this component
/// - **TargetPosition**: Resource coordination for click-to-move functionality
/// - **VisionSource**: Usually combined with vision capabilities
/// - **Player**: Often combined for player character identification
///
/// # Performance Impact
/// - **Input Processing**: Arrow key polling every frame
/// - **Mouse Handling**: Ray casting for world position conversion
/// - **Movement Calculation**: Vector math for smooth interpolation
#[derive(Component)]
struct MovableVision;

/// Marker component for entities that should continuously rotate around their Z-axis.
/// 应该围绕其Z轴连续旋转的实体的标记组件
///
/// This component identifies entities that should have automatic rotation
/// animation applied. Used for visual variety and to test fog of war
/// behavior with moving/changing entities.
///
/// # Rotation Properties
/// - **Rotation Rate**: π/2 radians per second (90 degrees per second)
/// - **Axis**: Z-axis rotation (2D plane rotation)
/// - **Continuity**: Smooth, continuous rotation without stopping
///
/// # Visual Effects
/// - **Entity Animation**: Provides visual movement for static entities
/// - **Fog Testing**: Tests how fog responds to entity orientation changes
/// - **Scene Dynamics**: Adds life to otherwise static scene elements
///
/// # System Integration
/// Processed by the `rotate_entities_system` which applies time-based
/// rotation to all entities marked with this component.
///
/// # Performance Characteristics
/// - **CPU Cost**: Minimal trigonometric calculations per entity
/// - **Memory**: No additional data, just marker component
/// - **Update Frequency**: Every frame for smooth rotation
#[derive(Component)]
struct RotationAble;

/// Component for entities that move horizontally back and forth within defined boundaries.
/// 在定义边界内水平来回移动的实体的组件
///
/// This component enables automatic horizontal movement with collision detection
/// at predefined boundaries. The entity bounces between left and right limits,
/// creating predictable patrol-like behavior.
///
/// # Movement Parameters
/// - **Speed**: 150.0 units per second (configurable in system)
/// - **Boundaries**: -450.0 (left) to +450.0 (right) world units
/// - **Direction**: 1.0 for rightward, -1.0 for leftward movement
/// - **Collision**: Instant direction reversal at boundaries
///
/// # Behavior Pattern
/// ```text
/// [Left Boundary] ←→ Entity Movement ←→ [Right Boundary]
///      -450.0              ↕                +450.0
///                    Direction Reversal
/// ```
///
/// # Use Cases
/// - **Moving Targets**: Creates dynamic entities for fog testing
/// - **Scene Animation**: Adds movement to otherwise static scenes
/// - **Interaction Testing**: Tests fog behavior with predictable movement patterns
///
/// # System Integration
/// Processed by `horizontal_movement_system` which handles:
/// - Position updates based on direction and speed
/// - Boundary collision detection and direction reversal
/// - Smooth movement with delta-time compensation
///
/// # Performance Characteristics
/// - **Update Frequency**: Every frame for smooth movement
/// - **CPU Cost**: Simple arithmetic operations per entity
/// - **Memory**: Single f32 for direction state
#[derive(Component)]
struct HorizontalMover {
    /// Movement direction: 1.0 for rightward, -1.0 for leftward movement.
    /// 移动方向：1.0表示向右移动，-1.0表示向左移动
    direction: f32,
}

/// Horizontal extent for distributing geometric shapes across the scene.
/// 在场景中分布几何形状的水平范围
///
/// This constant defines the total width over which various geometric shapes
/// are distributed during scene setup. Shapes are evenly spaced from
/// -X_EXTENT/2 to +X_EXTENT/2 in world coordinates.
///
/// # Usage
/// - **Shape Distribution**: Spreads 10 different geometric shapes evenly
/// - **World Layout**: Defines the horizontal span of the demo scene
/// - **Coordinate System**: Centered around world origin (0, 0)
///
/// # Mathematical Distribution
/// For n shapes, shape i is positioned at:
/// ```
/// x = -X_EXTENT/2 + i / (n-1) * X_EXTENT
/// ```
/// This creates even spacing across the full extent.
const X_EXTENT: f32 = 900.;

/// Sets up the initial scene with camera, entities, and vision sources for the fog of war demo.
/// 为战争迷雾演示设置初始场景，包括相机、实体和视野源
///
/// This function creates a comprehensive test environment with multiple entity types:
/// 1. **Camera Setup**: 2D camera with fog rendering capabilities
/// 2. **Text Elements**: Debug and information displays
/// 3. **Vision Sources**: Multiple entities with different vision configurations
/// 4. **Capturable Entities**: Objects that appear in fog snapshots
/// 5. **Geometric Shapes**: Array of different shapes for visual variety
///
/// # Entity Categories
///
/// ## Camera Entity
/// - **Components**: Camera2d, FogMaterialComponent, FogOfWarCamera
/// - **Purpose**: Primary viewpoint with fog rendering integration
///
/// ## Vision Source Entities
/// - **Gold Square**: Static 40-range square vision at (0, -50)
/// - **Player Circle**: Movable 100-range circle vision at (-200, -200)
/// - **Shape Cones**: Odd-indexed shapes with cone vision, varying ranges
///
/// ## Capturable Entities
/// - **Cyan Rotator**: Rotating entity at (-200, -50) for snapshot testing
/// - **Purple Mover**: Horizontally moving entity at (-400, -100)
/// - **Even Shapes**: Even-indexed geometric shapes for snapshot capture
///
/// ## Geometric Shapes Array
/// Creates 10 different shapes distributed across X_EXTENT:
/// - Circle, CircularSector, CircularSegment, Ellipse, Annulus
/// - Capsule2d, Rhombus, Rectangle, RegularPolygon, Triangle2d
///
/// # Performance Characteristics
/// - **Entity Count**: ~15 total entities (reasonable for demo)
/// - **Vision Sources**: 6 active vision sources with different configurations
/// - **Memory Usage**: Minimal geometric mesh allocations
/// - **GPU Impact**: Multiple vision calculations, but within acceptable limits
///
/// # Asset Dependencies
/// - **Font**: "fonts/FiraSans-Bold.ttf" for text rendering
/// - **Materials**: ColorMaterial instances for shape rendering
/// - **Meshes**: Various 2D geometric shapes from Bevy's primitives
///
/// # Color Scheme
/// - **Vision Sources**: Gold (square), cyan (player), HSL rainbow (cones)
/// - **Capturable**: Cyan and purple for visual distinction
/// - **Shapes**: HSL rainbow distribution for visual variety
fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let font_handle = asset_server.load("fonts/FiraSans-Bold.ttf");
    // 生成相机
    // Spawn camera
    commands.spawn((
        Camera2d,
        // 添加标记组件，以便稍后可以查询到此实体以添加/删除 FogMaterial
        // Add a marker component so we can query this entity later to add/remove FogMaterial
        FogMaterialComponent,
        FogOfWarCamera,
    ));

    commands.spawn((
        Text2d("Count".to_string()),
        TextFont {
            font: font_handle.clone(),
            font_size: 20.0,
            ..Default::default()
        },
        TextColor(RED.into()),
        Transform::from_translation(Vec3::new(200.0, -50.0, 0.0)),
        CountText,
    ));

    // 生成额外的视野提供者
    // Spawn additional vision providers
    commands.spawn((
        Sprite {
            color: GOLD.into(),
            custom_size: Some(Vec2::new(80.0, 80.0)),
            ..default()
        },
        Transform::from_translation(Vec3::new(0.0, -50.0, 0.0)),
        VisionSource {
            range: 40.0,
            enabled: true,
            shape: VisionShape::Square,
            direction: 0.0,
            angle: std::f32::consts::FRAC_PI_2,
            intensity: 1.0,
            transition_ratio: 0.2,
        },
    ));

    commands.spawn((
        Sprite {
            color: Color::srgb(0.2, 0.8, 0.8),
            custom_size: Some(Vec2::new(60.0, 60.0)),
            ..default()
        },
        Transform::from_translation(Vec3::new(-200.0, -50.0, 0.0)),
        Capturable,
        RotationAble,
    ));

    // 生成可移动的视野提供者（玩家）
    // Spawn movable vision provider (player)
    commands.spawn((
        Sprite {
            color: Color::srgb(0.0, 0.8, 0.8),
            custom_size: Some(Vec2::new(60.0, 60.0)),
            ..default()
        },
        Transform::from_translation(Vec3::new(-200.0, -200.0, 0.0)),
        VisionSource {
            range: 100.0,
            enabled: true,
            shape: VisionShape::Circle,
            direction: 0.0,
            angle: std::f32::consts::FRAC_PI_2,
            intensity: 1.0,
            transition_ratio: 0.2,
        },
        MovableVision,
        Player,
    ));

    // 生成水平来回移动的 Sprite
    // Spawn horizontally moving sprite
    commands.spawn((
        Sprite {
            color: Color::srgb(0.9, 0.1, 0.9), // 紫色 / Purple color
            custom_size: Some(Vec2::new(50.0, 50.0)),
            ..default()
        },
        Transform::from_translation(Vec3::new(-400.0, -100.0, 0.0)), // 初始位置 / Initial position
        HorizontalMover { direction: 1.0 }, // 初始向右移动 / Initially move right
    ));

    let shapes = [
        meshes.add(Circle::new(50.0)),
        meshes.add(CircularSector::new(50.0, 1.0)),
        meshes.add(CircularSegment::new(50.0, 1.25)),
        meshes.add(Ellipse::new(25.0, 50.0)),
        meshes.add(Annulus::new(25.0, 50.0)),
        meshes.add(Capsule2d::new(25.0, 50.0)),
        meshes.add(Rhombus::new(75.0, 100.0)),
        meshes.add(Rectangle::new(50.0, 100.0)),
        meshes.add(RegularPolygon::new(50.0, 6)),
        meshes.add(Triangle2d::new(
            Vec2::Y * 50.0,
            Vec2::new(-50.0, -50.0),
            Vec2::new(50.0, -50.0),
        )),
    ];
    let num_shapes = shapes.len();

    for (i, shape) in shapes.into_iter().enumerate() {
        // Distribute colors evenly across the rainbow.
        let color = Color::hsl(360. * i as f32 / num_shapes as f32, 0.95, 0.7);

        let mut entity_commands = commands.spawn((
            Mesh2d(shape),
            MeshMaterial2d(materials.add(color)),
            Transform::from_xyz(
                // Distribute shapes from -X_EXTENT/2 to +X_EXTENT/2.
                -X_EXTENT / 2. + i as f32 / (num_shapes - 1) as f32 * X_EXTENT,
                100.0,
                0.0,
            ),
        ));

        // 为偶数索引的方块添加视野提供者
        // Add vision provider to blocks with even indices
        if i.is_multiple_of(2) {
            entity_commands.insert(Capturable);
        } else {
            entity_commands.insert((
                VisionSource {
                    range: 30.0 + (i as f32 * 15.0),
                    enabled: true,
                    shape: VisionShape::Cone,
                    direction: (i as f32 * 75.0),
                    angle: std::f32::consts::FRAC_PI_2,
                    intensity: 1.0,
                    transition_ratio: 0.2,
                },
                RotationAble,
            ));
        }
    }
}

/// System for handling camera movement via keyboard input (WASD keys).
/// 处理通过键盘输入（WASD键）进行相机移动的系统
///
/// This system enables smooth camera movement for exploring the fog of war world.
/// Players can move the camera in all four cardinal directions using WASD keys,
/// with movement speed and direction calculated based on elapsed time.
///
/// # Controls
/// - **W**: Move camera up (positive Y direction)
/// - **A**: Move camera left (negative X direction)
/// - **S**: Move camera down (negative Y direction)
/// - **D**: Move camera right (positive X direction)
/// - **Diagonal**: Multiple key combinations for diagonal movement
///
/// # Movement Mechanics
/// - **Speed**: 500.0 units per second
/// - **Normalization**: Diagonal movement normalized to prevent speed boost
/// - **Time-based**: Uses delta time for frame-rate independent movement
/// - **Smooth**: Continuous movement while keys are held
///
/// # Performance Characteristics
/// - **Input Polling**: Checks 4 keys every frame
/// - **Vector Math**: Simple addition and normalization operations
/// - **Transform Update**: Single transform modification per frame
/// - **Time Complexity**: O(1) per frame
///
/// # Integration Points
/// - **FogOfWarCamera**: Queries specifically for the fog camera entity
/// - **Fog System**: Camera movement triggers fog chunk loading/unloading
/// - **Exploration**: Moving camera reveals new areas for fog of war
///
/// # Future Enhancements
/// The commented code shows potential mouse-edge movement implementation
/// for RTS-style camera controls, which could be enabled in future versions.
fn camera_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut camera_query: Query<&mut Transform, With<FogOfWarCamera>>,
    _window_query: Query<&Window>,
) {
    if let Ok(mut camera_transform) = camera_query.single_mut() {
        let mut direction = Vec3::ZERO;
        let speed = 500.0; // 移动速度 / Movement speed

        // WASD 键控制移动
        // WASD keys control movement
        if keyboard.pressed(KeyCode::KeyW) {
            direction.y += 1.0; // 向上移动 / Move up
        }
        if keyboard.pressed(KeyCode::KeyS) {
            direction.y -= 1.0; // 向下移动 / Move down
        }
        if keyboard.pressed(KeyCode::KeyA) {
            direction.x -= 1.0; // 向左移动 / Move left
        }
        if keyboard.pressed(KeyCode::KeyD) {
            direction.x += 1.0; // 向右移动 / Move right
        }

        // // 获取主窗口和鼠标位置
        // // Get primary window and mouse position
        // if let Ok(window) = window_query.get_single() {
        //     if let Some(mouse_pos) = window.cursor_position() {
        //         let window_width = window.width();
        //         let window_height = window.height();
        //
        //         // 定义边缘区域的大小（占窗口尺寸的百分比）
        //         // Define edge zone size (as a percentage of window dimensions)
        //         let edge_zone_percent = 0.05;
        //         let edge_size_x = window_width * edge_zone_percent;
        //         let edge_size_y = window_height * edge_zone_percent;
        //
        //         // 计算边缘区域的边界
        //         // Calculate edge zone boundaries
        //         let left_edge = edge_size_x;
        //         let right_edge = window_width - edge_size_x;
        //         let top_edge = edge_size_y;
        //         let bottom_edge = window_height - edge_size_y;
        //
        //         // 根据鼠标位置判断移动方向
        //         // Determine movement direction based on mouse position
        //         if mouse_pos.x < left_edge {
        //             direction.x -= 1.0; // 左移 / Move left
        //         }
        //         if mouse_pos.x > right_edge {
        //             direction.x += 1.0; // 右移 / Move right
        //         }
        //         if mouse_pos.y < top_edge {
        //             direction.y += 1.0; // 上移 / Move up
        //         }
        //         if mouse_pos.y > bottom_edge {
        //             direction.y -= 1.0; // 下移 / Move down
        //         }
        //     }
        // }

        if direction != Vec3::ZERO {
            direction = direction.normalize();
            camera_transform.translation += direction * speed * time.delta_secs();
        }
    }
}

/// System for updating fog of war settings based on keyboard input.
/// 基于键盘输入更新战争迷雾设置的系统
///
/// This system provides real-time control over fog rendering properties,
/// allowing users to toggle fog visibility and adjust transparency levels
/// during runtime for testing and demonstration purposes.
///
/// # Controls
/// - **F Key**: Toggle fog enabled/disabled state
/// - **Page Up**: Increase fog transparency (make fog more opaque)
/// - **Page Down**: Decrease fog transparency (make fog more transparent)
///
/// # Fog Properties Modified
/// - **enabled**: Boolean flag controlling overall fog rendering
/// - **fog_color_unexplored.alpha**: Transparency of unexplored areas (0.0-1.0)
///
/// # UI Integration
/// Updates the fog settings text display with:
/// - Current enabled/disabled status
/// - Current alpha percentage (0-100%)
/// - Control instructions for user reference
///
/// # Performance Characteristics
/// - **Input Processing**: Minimal key state checking
/// - **Setting Updates**: Direct resource modification
/// - **UI Updates**: String formatting only when changes occur
/// - **Alpha Adjustment**: Smooth 0.5 units per second change rate
///
/// # Real-time Effects
/// Changes take effect immediately:
/// - **Toggle**: Fog disappears/appears instantly
/// - **Alpha**: Transparency changes smoothly over frames
/// - **Visual Feedback**: UI text updates reflect current state
///
/// # Clamping
/// Alpha values are properly clamped to [0.0, 1.0] range to prevent
/// invalid transparency values that could cause rendering issues.
fn update_fog_settings(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut fog_settings: ResMut<FogMapSettings>,
    mut settings_text_query: Query<&mut Text, With<FogSettingsText>>,
) {
    if keyboard.just_pressed(KeyCode::KeyF) {
        fog_settings.enabled = !fog_settings.enabled;
    }

    // 更新雾颜色透明度
    // Update fog color alpha
    if keyboard.pressed(KeyCode::PageUp) {
        let new_alpha =
            (fog_settings.fog_color_unexplored.alpha() + time.delta_secs() * 0.5).min(1.0);
        fog_settings.fog_color_unexplored.set_alpha(new_alpha);
    }
    if keyboard.pressed(KeyCode::PageDown) {
        let new_alpha =
            (fog_settings.fog_color_unexplored.alpha() - time.delta_secs() * 0.5).max(0.0);
        fog_settings.fog_color_unexplored.set_alpha(new_alpha);
    }

    // 更新 UI 文本
    // Update UI text
    if let Ok(mut text) = settings_text_query.single_mut() {
        let alpha_percentage = fog_settings.fog_color_unexplored.alpha() * 100.0;
        let status = if fog_settings.enabled {
            "Enabled"
        } else {
            "Disabled"
        };
        text.0 = format!(
            "Fog Status: {status}\nPress F to toggle\nPress Up/Down to adjust Alpha: {alpha_percentage:.0}%"
        );
    }
}

/// Creates and configures the user interface elements for the fog of war demo.
/// 为战争迷雾演示创建和配置用户界面元素
///
/// This system sets up all UI text elements that provide information and
/// feedback to the user during the demo. UI elements are positioned using
/// absolute positioning for precise layout control.
///
/// # UI Elements Created
///
/// ## FPS Display (Top-Left)
/// - **Position**: 10px from top and left edges
/// - **Content**: "FPS: {value}" with real-time frame rate
/// - **Color**: Medium gray for unobtrusive display
/// - **Structure**: Parent text + child span for dynamic value updates
///
/// ## Fog Settings Info (Below FPS)
/// - **Position**: 40px from top, 10px from left
/// - **Content**: Fog status, controls, and alpha percentage
/// - **Updates**: Real-time updates based on user input
/// - **Alignment**: Left-justified multi-line text
///
/// ## Control Instructions (Bottom-Left)
/// - **Position**: 20px from bottom, 10px from left
/// - **Content**: Complete list of keyboard controls
/// - **Purpose**: User reference for all available interactions
/// - **Style**: Smaller font, darker gray for reference info
///
/// ## Title Text (Bottom-Right)
/// - **Position**: 20px from bottom and right edges
/// - **Content**: "Fog of War" application title
/// - **Style**: Large 32px font, medium gray
/// - **Future**: Marked for color animation (ColorAnimatedText)
///
/// # Text Hierarchy
/// ```text
/// ┌────────────┐               ┌─────────┐
/// │ FPS: 60.0   │               │         │
/// │ Fog: On     │               │         │
/// │ Alpha: 75%  │               │         │
/// │            │      ...      │         │
/// │            │               │         │
/// │            │               │         │
/// │ Controls:   │               │ Fog of  │
/// │ WASD-Move   │               │   War   │
/// └────────────┘               └─────────┘
/// ```
///
/// # Performance Characteristics
/// - **One-time Setup**: Called only during application startup
/// - **Memory**: Minimal text allocation for UI strings
/// - **Rendering**: Efficient UI rendering via Bevy's built-in text system
/// - **Updates**: Only dynamic elements (FPS, settings) update after creation
///
/// # Accessibility
/// - **Readable Fonts**: Uses FiraSans-Bold for clear text rendering
/// - **Contrast**: Medium/dark gray colors provide good contrast on white background
/// - **Size**: Appropriate font sizes for different information types
/// - **Positioning**: Non-overlapping layout with clear visual hierarchy
fn setup_ui(mut commands: Commands) {
    // 创建 FPS 显示文本
    // Create FPS display text
    commands
        .spawn((
            // 创建一个带有多个部分的文本
            // Create a Text with multiple sections
            Text::new("FPS: "),
            TextFont {
                font_size: 24.0,
                ..default()
            },
            // 设置节点样式
            // Set node style
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(10.0),
                ..default()
            },
            // 设置为中灰色
            // Set to medium gray
            TextColor(Color::srgb(0.5, 0.5, 0.5)),
        ))
        .with_child((
            TextSpan::default(),
            TextFont {
                font_size: 24.0,
                ..default()
            },
            // 设置为中灰色
            // Set to medium gray
            TextColor(Color::srgb(0.5, 0.5, 0.5)),
            FpsText,
        ));

    // 创建迷雾设置显示文本
    // Create fog settings display text
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextLayout::new_with_justify(JustifyText::Left),
        // 设置为中灰色
        // Set to medium gray
        TextColor(Color::srgb(0.5, 0.5, 0.5)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(40.0),
            left: Val::Px(10.0),
            ..default()
        },
        FogSettingsText,
    ));

    // 创建控制说明文本
    // Create control instructions text
    commands.spawn((
        Text::new(
            "Controls:\n\
             WASD - Move camera\n\
             Arrow Keys - Move blue vision source\n\
             F - Toggle fog\n\
             R - Reset fog of war\n\
             PageUp/Down - Adjust fog alpha\n\
             Left Click - Set target for blue vision source\n\
             P - Save fog data (best format auto-selected)\n\
             L - Load fog data (auto-detects format)\n\
             F12 - Force snapshot all Capturable entities on screen\n\
             Automatic format selection & compression",
        ),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextLayout::new_with_justify(JustifyText::Left),
        TextColor(Color::srgb(0.4, 0.4, 0.4)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(20.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));

    // 创建颜色动画标题文本
    // Create color animated title text
    commands.spawn((
        Text::new("Fog of War"),
        TextFont {
            font_size: 32.0,
            ..default()
        },
        // 设置为中灰色
        // Set to medium gray
        TextColor(Color::srgb(0.5, 0.5, 0.5)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(20.0),
            right: Val::Px(20.0),
            ..default()
        },
        ColorAnimatedText,
    ));
}

/// System that updates the FPS display text with current frame rate information.
/// 使用当前帧率信息更新FPS显示文本的系统
///
/// This system reads frame time diagnostics from Bevy and updates the FPS
/// display text with smoothed frame rate values. Provides real-time performance
/// monitoring for users and developers.
///
/// # Data Source
/// - **DiagnosticsStore**: Bevy's built-in performance monitoring system
/// - **FrameTimeDiagnosticsPlugin::FPS**: Specific FPS diagnostic metric
/// - **Smoothed Values**: Uses smoothed average rather than instantaneous FPS
///
/// # Update Process
/// 1. **Query Entities**: Find all text spans marked with FpsText component
/// 2. **Read Diagnostics**: Access current FPS data from diagnostics store
/// 3. **Extract Value**: Get smoothed FPS value if available
/// 4. **Format Text**: Convert to string with 1 decimal place precision
/// 5. **Update Display**: Set text span content to formatted FPS value
///
/// # Display Format
/// - **Precision**: One decimal place (e.g., "60.1")
/// - **Units**: Frames per second (implicit, not displayed)
/// - **Fallback**: Text unchanged if FPS data unavailable
///
/// # Performance Characteristics
/// - **Update Frequency**: Every frame (60+ times per second)
/// - **CPU Cost**: Minimal diagnostic lookup and string formatting
/// - **Memory**: Small string allocation per update
/// - **Time Complexity**: O(n) where n = number of FPS text elements
///
/// # Error Handling
/// - **Missing Diagnostics**: Silently continues without updating text
/// - **No FPS Data**: Preserves previous text content
/// - **Multiple Elements**: Handles multiple FPS displays if present
///
/// # Integration Points
/// - **FpsText Component**: Targets specific UI elements for updates
/// - **DiagnosticsStore**: Reads from Bevy's performance monitoring
/// - **TextSpan**: Updates dynamic text content within UI hierarchy
fn update_fps_text(
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<&mut TextSpan, With<FpsText>>,
) {
    for mut span in &mut query {
        if let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS) {
            if let Some(value) = fps.smoothed() {
                // 更新 FPS 文本值
                // Update FPS text value
                **span = format!("{value:.1}");
            }
        }
    }
}

/// System that updates count display text with current frame number.
/// 使用当前帧号更新计数显示文本的系统
///
/// This system provides a simple frame counter display that shows how many
/// frames have been rendered since application start. Useful for debugging,
/// timing analysis, and tracking application runtime.
///
/// # Data Source
/// - **FrameCount**: Bevy's built-in resource tracking total frames rendered
/// - **Incremental**: Counter increases by 1 every frame
/// - **Persistent**: Maintains count throughout application lifetime
///
/// # Display Format
/// - **Format**: "Count: {frame_number}"
/// - **Example**: "Count: 3847"
/// - **Type**: Text2d for world-space rendering
///
/// # Performance Characteristics
/// - **Update Frequency**: Every frame
/// - **CPU Cost**: Minimal string formatting
/// - **Memory**: Small string allocation per frame
/// - **Time Complexity**: O(n) where n = number of count text elements
///
/// # Use Cases
/// - **Debug Information**: Track frame progression during testing
/// - **Performance Correlation**: Correlate events with specific frame numbers
/// - **Runtime Tracking**: Monitor how long application has been running
/// - **Animation Reference**: Frame-based timing for animations or events
///
/// # Integration
/// - **CountText Component**: Identifies which text elements to update
/// - **Text2d**: World-space text rendering system
/// - **FrameCount Resource**: Bevy's internal frame counting system
fn update_count_text(mut query: Query<&mut Text2d, With<CountText>>, frame_count: Res<FrameCount>) {
    for mut text in &mut query {
        text.0 = format!("Count: {}", frame_count.0);
    }
}

/// System for controlling player movement via keyboard input and mouse click-to-move.
/// 通过键盘输入和鼠标点击移动控制玩家移动的系统
///
/// This system provides dual control mechanisms for the player entity:
/// immediate keyboard control and smooth click-to-move targeting.
/// It handles input processing, coordinate conversion, and smooth movement interpolation.
///
/// # Control Methods
///
/// ## Keyboard Controls (Arrow Keys)
/// - **↑ Up**: Move north (positive Y)
/// - **↓ Down**: Move south (negative Y)
/// - **← Left**: Move west (negative X)
/// - **→ Right**: Move east (positive X)
/// - **Immediate**: Direct position updates, cancels mouse targets
///
/// ## Mouse Click-to-Move
/// - **Left Click**: Set target position in world coordinates
/// - **Smooth Movement**: Gradual interpolation toward target
/// - **Cancellation**: Arrow key input cancels current mouse target
/// - **Precision**: 5.0 unit tolerance for reaching targets
///
/// # Movement Mechanics
/// - **Speed**: 200.0 units per second for both control methods
/// - **Normalization**: Direction vectors normalized to prevent speed variations
/// - **Delta Time**: Frame-rate independent movement calculations
/// - **Overshoot Protection**: Prevents moving past click targets
///
/// # Coordinate Conversion
/// Mouse clicks undergo screen-to-world coordinate conversion:
/// 1. **Screen Position**: Mouse cursor position in window pixels
/// 2. **Camera Query**: Get camera and transform for ray casting
/// 3. **Viewport Ray**: Convert screen coordinates to world ray
/// 4. **2D Projection**: Extract X,Y coordinates for 2D movement
/// 5. **Target Storage**: Store world position in TargetPosition resource
///
/// # State Management
/// - **TargetPosition Resource**: Shared state for click-to-move targets
/// - **Priority System**: Keyboard input overrides mouse targets
/// - **Persistence**: Targets persist until reached or cancelled
///
/// # Performance Characteristics
/// - **Input Processing**: Arrow key polling every frame
/// - **Mouse Handling**: Ray casting only when clicked
/// - **Movement Math**: Vector operations for interpolation
/// - **Query Efficiency**: Single entity queries for player and camera
/// - **Time Complexity**: O(1) per frame
///
/// # Integration Points
/// - **MovableVision Component**: Identifies controllable entities
/// - **TargetPosition Resource**: Global state for click targets
/// - **FogOfWarCamera**: Camera used for coordinate conversion
/// - **VisionSource**: Usually combined for exploration capabilities
fn movable_vision_control(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mouse_button_input: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<FogOfWarCamera>>,
    mut query: Query<&mut Transform, With<MovableVision>>,
    mut target_position: ResMut<TargetPosition>,
) {
    if let Ok(mut transform) = query.single_mut() {
        let mut movement = Vec3::ZERO;
        let speed = 200.0; // 移动速度 / Movement speed
        let dt = time.delta_secs();

        // 箭头键控制移动
        // Arrow keys control movement
        if keyboard.pressed(KeyCode::ArrowUp) {
            movement.y += speed * dt; // 向上移动 / Move up
            target_position.0 = None; // 取消鼠标目标点 / Cancel mouse target
        }
        if keyboard.pressed(KeyCode::ArrowDown) {
            movement.y -= speed * dt; // 向下移动 / Move down
            target_position.0 = None; // 取消鼠标目标点 / Cancel mouse target
        }
        if keyboard.pressed(KeyCode::ArrowLeft) {
            movement.x -= speed * dt; // 向左移动 / Move left
            target_position.0 = None; // 取消鼠标目标点 / Cancel mouse target
        }
        if keyboard.pressed(KeyCode::ArrowRight) {
            movement.x += speed * dt; // 向右移动 / Move right
            target_position.0 = None; // 取消鼠标目标点 / Cancel mouse target
        }

        // 处理鼠标点击事件
        // Handle mouse click event
        if mouse_button_input.just_pressed(MouseButton::Left) {
            if let Ok(window) = windows.single() {
                if let Some(cursor_position) = window.cursor_position() {
                    // 获取摄像机和全局变换
                    // Get camera and global transform
                    if let Ok((camera, camera_transform)) = cameras.single() {
                        // 将屏幕坐标转换为世界坐标
                        // Convert screen coordinates to world coordinates
                        if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position)
                        {
                            // 处理 2D 平面上的目标点
                            // Handle target point on 2D plane
                            // 为简单起见，直接使用原始 x,y 坐标
                            // For simplicity, directly use original x,y coordinates
                            let target_pos =
                                Vec3::new(ray.origin.x, ray.origin.y, transform.translation.z);

                            // 设置移动目标点
                            // Set movement target point
                            target_position.0 = Some(target_pos);
                        }
                    }
                }
            }
        }

        // 如果有目标位置，则向目标位置平滑移动
        // If there is a target position, smoothly move towards it
        if let Some(target) = target_position.0 {
            let direction = target - transform.translation;
            let distance = direction.length();

            // 如果距离足够小，则认为已经到达目标
            // If distance is small enough, consider target reached
            if distance < 5.0 {
                target_position.0 = None;
            } else {
                // 计算这一帧的移动距离，使用标准化的方向和速度
                // Calculate movement for this frame using normalized direction and speed
                let move_dir = direction.normalize();
                let move_amount = speed * dt;

                // 确保不会超过目标位置
                // Ensure we don't overshoot the target
                let actual_move = if move_amount > distance {
                    direction
                } else {
                    move_dir * move_amount
                };

                // 应用移动
                // Apply movement
                movement = actual_move;
            }
        }

        // 应用移动
        // Apply movement
        transform.translation += movement;
    }
}

/// System that handles automatic horizontal back-and-forth movement for patrol entities.
/// 处理巡逻实体自动水平来回移动的系统
///
/// This system creates predictable patrol behavior for entities marked with
/// HorizontalMover component. Entities move back and forth between defined
/// boundaries, reversing direction when limits are reached.
///
/// # Movement Parameters
/// - **Speed**: 150.0 units per second
/// - **Left Boundary**: -450.0 world units
/// - **Right Boundary**: +450.0 world units
/// - **Total Range**: 900.0 units of movement space
///
/// # Collision Behavior
/// When an entity reaches a boundary:
/// 1. **Position Clamping**: Entity position set exactly to boundary value
/// 2. **Direction Reversal**: Direction multiplied by -1
/// 3. **Immediate Effect**: Direction change takes effect next frame
/// 4. **No Overshoot**: Prevents entity from moving beyond boundaries
///
/// # Movement Pattern
/// ```text
/// [-450] ←───────── Entity ─────────→ [+450]
///   ↑                                           ↑
///   Reverse                                   Reverse
///   direction                                 direction
/// ```
///
/// # Performance Characteristics
/// - **Update Frequency**: Every frame for smooth movement
/// - **CPU Cost**: Simple arithmetic operations per entity
/// - **Entity Count**: Scales linearly with number of HorizontalMover entities
/// - **Boundary Checks**: Two comparisons per entity per frame
/// - **Time Complexity**: O(n) where n = number of horizontal movers
///
/// # Use Cases
/// - **Moving Targets**: Creates dynamic entities for fog interaction testing
/// - **Scene Animation**: Adds movement to otherwise static scenes
/// - **Predictable Patterns**: Reliable movement for testing fog behavior
/// - **Visual Interest**: Provides continuous animation without player input
///
/// # Integration Points
/// - **HorizontalMover Component**: Identifies entities for this system
/// - **Transform Component**: Modified for position updates
/// - **Fog System**: Moving entities trigger fog updates as they explore
/// - **Capturable Entities**: Often combined for snapshot testing with movement
fn horizontal_movement_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut HorizontalMover)>,
) {
    let speed = 150.0; // 移动速度 / Movement speed
    let left_bound = -450.0; // 左边界 / Left boundary
    let right_bound = 450.0; // 右边界 / Right boundary

    for (mut transform, mut mover) in query.iter_mut() {
        // 根据方向和速度更新位置
        // Update position based on direction and speed
        transform.translation.x += mover.direction * speed * time.delta_secs();

        // 检查是否到达边界，如果到达则反转方向
        // Check if boundaries are reached, reverse direction if so
        if transform.translation.x >= right_bound {
            transform.translation.x = right_bound; // 防止超出边界 / Prevent exceeding boundary
            mover.direction = -1.0; // 向左移动 / Move left
        } else if transform.translation.x <= left_bound {
            transform.translation.x = left_bound; // 防止超出边界 / Prevent exceeding boundary
            mover.direction = 1.0; // 向右移动 / Move right
        }
    }
}

/// System that applies continuous Z-axis rotation to entities marked with RotationAble.
/// 对标记为RotationAble的实体应用连续 Z轴旋转的系统
///
/// This system provides smooth, continuous rotation animation for entities
/// that should spin around their center point. Used for visual variety
/// and to test fog of war behavior with changing entity orientations.
///
/// # Rotation Properties
/// - **Rotation Rate**: π/2 radians per second (90 degrees per second)
/// - **Axis**: Z-axis rotation (clockwise when viewed from positive Z)
/// - **Continuity**: Smooth rotation without pauses or direction changes
/// - **Time-based**: Frame-rate independent using delta time
///
/// # Mathematical Details
/// ```rust
/// rotation_per_frame = FRAC_PI_2 * delta_seconds
/// // At 60 FPS: π/2 * (1/60) = ~0.026 radians per frame
/// // Full rotation takes: 2π / (π/2) = 4 seconds
/// ```
///
/// # Visual Effects
/// - **Entity Animation**: Provides continuous visual movement
/// - **Fog Testing**: Tests how fog responds to entity orientation changes
/// - **Scene Dynamics**: Adds life to otherwise static scene elements
/// - **Recognition**: Helps identify which entities have this behavior
///
/// # Performance Characteristics
/// - **Update Frequency**: Every frame for smooth rotation
/// - **CPU Cost**: Simple trigonometric function call per entity
/// - **Memory**: No additional allocations, just transform updates
/// - **Time Complexity**: O(n) where n = number of RotationAble entities
/// - **GPU Impact**: Transform changes trigger render updates
///
/// # Integration Points
/// - **RotationAble Component**: Identifies entities for rotation
/// - **Transform Component**: Modified for rotation updates
/// - **Rendering System**: Rotated transforms affect visual rendering
/// - **Fog System**: Rotating entities may affect fog calculations if they're vision sources
///
/// # Use Cases
/// - **Visual Polish**: Adds professional animation quality
/// - **Testing**: Verifies fog system handles dynamic entity states
/// - **Identification**: Makes certain entities easily recognizable
/// - **Performance Testing**: Provides consistent transform update load
fn rotate_entities_system(time: Res<Time>, mut query: Query<&mut Transform, With<RotationAble>>) {
    for mut transform in query.iter_mut() {
        transform.rotate_z(std::f32::consts::FRAC_PI_2 * time.delta_secs()); // 90 degrees per second / 每秒旋转90度
    }
}

/// System that monitors and logs fog reset operation results.
/// 监控和记录雾效重置操作结果的系统
///
/// This system provides comprehensive logging and user feedback for fog reset
/// operations, handling both successful completions and failure scenarios.
/// It processes events from the fog reset system and provides informative
/// console output for debugging and user awareness.
///
/// # Event Types Handled
///
/// ## FogResetSuccess Events
/// - **Duration Logging**: Reports how long the reset operation took
/// - **Chunk Statistics**: Shows number of chunks that were reset
/// - **Success Confirmation**: Provides positive feedback with ✓ icon
/// - **Performance Data**: Helps identify reset performance patterns
///
/// ## FogResetFailed Events
/// - **Error Reporting**: Logs specific error details
/// - **Duration Tracking**: Shows partial completion time before failure
/// - **Failure Indication**: Provides clear error feedback with ✗ icon
/// - **Debug Information**: Helps troubleshoot reset issues
///
/// # Logging Format
/// ```text
/// Success: ✅ Fog reset completed successfully! Duration: 125ms, Chunks reset: 47
/// Failure: ❌ Fog reset failed! Duration: 89ms, Error: Texture reset failed: GPU allocation error
/// ```
///
/// # Performance Characteristics
/// - **Event Processing**: Minimal overhead, only when events occur
/// - **Logging Cost**: String formatting and console output
/// - **Memory**: Temporary string allocations for log messages
/// - **Frequency**: Infrequent, only during reset operations
///
/// # Integration Points
/// - **FogResetSuccess**: Successful reset completion events
/// - **FogResetFailed**: Failed reset attempt events
/// - **Logging System**: Uses Bevy's info! and error! macros
/// - **User Feedback**: Provides immediate feedback for reset operations
///
/// # Debug Value
/// - **Performance Monitoring**: Track reset operation timing
/// - **Error Diagnosis**: Detailed error information for troubleshooting
/// - **User Experience**: Clear feedback about operation status
/// - **Development**: Helps optimize reset system performance
///
/// # Error Categories
/// The system can log various error types:
/// - Texture reset failures
/// - GPU memory issues
/// - Synchronization problems
/// - Timeout errors
/// - Unknown system failures
fn handle_fog_reset_events(
    mut success_events: EventReader<FogResetSuccess>,
    mut failure_events: EventReader<FogResetFailed>,
) {
    for event in success_events.read() {
        info!(
            "✅ Fog reset completed successfully! Duration: {}ms, Chunks reset: {}",
            event.duration_ms, event.chunks_reset
        );
    }

    for event in failure_events.read() {
        error!(
            "❌ Fog reset failed! Duration: {}ms, Error: {}",
            event.duration_ms, event.error
        );
    }
}

/// System that provides visual debugging of fog of war chunk system.
/// 提供战争迷雾区块系统可视化调试的系统
///
/// This comprehensive debugging system visualizes the internal state of the
/// fog of war chunk system when fog rendering is disabled. It provides
/// essential information for developers and advanced users to understand
/// how the chunk system operates.
///
/// # Debug Visualizations
///
/// ## Chunk Boundaries
/// - **Visual**: Red translucent rectangles outlining each chunk
/// - **Purpose**: Shows chunk spatial division of the world
/// - **Alpha**: 0.3 transparency to avoid obscuring scene content
/// - **Shape**: Rectangle matching exact chunk world bounds
///
/// ## Chunk Information Text
/// - **Snapshot Layer ID**: Which layer in snapshot texture array
/// - **Fog Layer ID**: Which layer in fog texture array  
/// - **Coordinates**: Chunk coordinates in chunk space (x, y)
/// - **Position**: Centered within each chunk boundary
/// - **Font**: FiraSans-Bold, 13px for readability
///
/// ## Statistics Display
/// Updates fog settings text with real-time chunk metrics:
/// - **Total Chunks**: Count of all active chunk entities
/// - **Chunks in Vision**: Number of chunks within camera view
/// - **Performance Insight**: Helps monitor chunk loading behavior
///
/// # Conditional Rendering
/// Debug visualization only appears when:
/// - **fog_settings.enabled == false**: Fog rendering is disabled
/// - **Debug Mode**: Prevents interference with normal fog rendering
/// - **Performance**: No overhead when fog is enabled
///
/// # Information Format
/// ```text
/// Chunk Text Display:
/// sid: Some(15)     // Snapshot layer index
/// lid: Some(23)     // Fog layer index  
/// (2, -1)          // Chunk coordinates
///
/// Statistics Addition:
/// [Previous fog settings text]
/// Total Chunks: 12
/// Chunks in Vision: 6
/// ```
///
/// # Performance Characteristics
/// - **Conditional Execution**: Only runs when fog disabled
/// - **Gizmo Rendering**: Efficient GPU debug drawing
/// - **Text Creation**: Dynamic text spawning for new chunks
/// - **Query Efficiency**: Single pass through chunk entities
/// - **Memory**: Temporary gizmo and text allocations
///
/// # Development Benefits
/// - **Chunk Visualization**: See chunk spatial organization
/// - **Layer Tracking**: Monitor texture array allocation
/// - **Performance Analysis**: Track chunk loading patterns
/// - **Memory Debugging**: Identify chunk creation/destruction
/// - **Spatial Understanding**: Visualize world-to-chunk mapping
///
/// # Asset Dependencies
/// - **Font**: "fonts/FiraSans-Bold.ttf" for debug text
/// - **Gizmos**: Bevy's debug drawing system
/// - **UI System**: Text component and styling systems
///
/// # Use Cases
/// - **Development**: Understanding chunk system behavior
/// - **Debugging**: Diagnosing chunk loading issues
/// - **Optimization**: Identifying unnecessary chunk creation
/// - **Education**: Learning how fog of war chunks work
fn debug_draw_chunks(
    mut gizmos: Gizmos,
    mut chunk_query: Query<(Entity, &FogChunk, Option<&mut Text2d>)>,
    cache: ResMut<ChunkStateCache>,
    fog_settings: Res<FogMapSettings>, // Access ChunkManager for tile_size
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut debug_text_query: Query<&mut Text, With<FogSettingsText>>,
) {
    // 计算所有chunk数量和视野内的chunk数量
    // Calculate total chunk count and chunks in vision
    let total_chunks = chunk_query.iter().count();
    let chunks_in_vision = cache.camera_view_chunks.len();

    // 更新调试文本以显示chunk数量
    // Update debug text to show chunk counts
    if let Ok(mut text) = debug_text_query.single_mut() {
        let current_text = text.0.clone();
        text.0 = format!(
            "{current_text}\nTotal Chunks: {total_chunks}\nChunks in Vision: {chunks_in_vision}"
        );
    }

    if !fog_settings.enabled {
        for (chunk_entity, chunk, opt_text) in chunk_query.iter_mut() {
            // Draw chunk boundary rectangle
            gizmos.rect_2d(
                chunk.world_bounds.center(),
                chunk.world_bounds.size(),
                RED.with_alpha(0.3),
            );
            if let Some(mut text) = opt_text {
                text.0 = format!(
                    "sid: {:?}\nlid: {:?}\n({}, {})",
                    chunk.snapshot_layer_index,
                    chunk.fog_layer_index,
                    chunk.coords.x,
                    chunk.coords.y
                );
            } else {
                let font = asset_server.load("fonts/FiraSans-Bold.ttf");
                let text_font = TextFont {
                    font: font.clone(),
                    font_size: 13.0,
                    ..default()
                };
                let pos = fog_settings.chunk_coord_to_world(chunk.coords)
                    + chunk.world_bounds.size() * 0.5;

                // Draw chunk unique_id and coordinate text
                // 显示区块 unique_id 和坐标的文本
                commands.entity(chunk_entity).insert((
                    Text2d::new(format!(
                        "sid: {:?}\nlid: {:?}\n({}, {})",
                        chunk.snapshot_layer_index,
                        chunk.fog_layer_index,
                        chunk.coords.x,
                        chunk.coords.y
                    )),
                    text_font,
                    TextColor(RED.into()),
                    Transform::from_translation(Vec3::new(pos.x, pos.y, 0.0)),
                ));
            }
        }
    }
}

/// System that monitors keyboard input for fog reset commands.
/// 监控键盘输入以获取雾效重置命令的系统
///
/// This system provides a simple keyboard interface for triggering complete
/// fog of war reset operations. When the user presses the R key, it initiates
/// a full reset that clears all explored areas and returns the fog to its
/// initial unexplored state.
///
/// # Controls
/// - **R Key**: Trigger complete fog of war reset
/// - **Just Pressed**: Only responds to key press, not held key
/// - **Immediate**: Reset event sent immediately upon key detection
///
/// # Reset Operation
/// When triggered, the system:
/// 1. **Logs Intent**: Info message about reset initiation
/// 2. **Sends Event**: ResetFogOfWar event to fog system
/// 3. **System Response**: Fog system handles complete reset process
/// 4. **User Feedback**: Log message provides immediate feedback
///
/// # Event Flow
/// ```text
/// [R Key Press] → [handle_reset_input] → [ResetFogOfWar Event] → [Fog System]
///       ↑                    ↓                      ↓                 ↓
///   User Input        Logs "Resetting..."       Event Queue      Full Reset
/// ```
///
/// # Performance Characteristics
/// - **Input Polling**: Checks R key state every frame
/// - **Event Frequency**: Very low - only when user presses R
/// - **CPU Cost**: Minimal key state checking
/// - **Memory**: Single event allocation when triggered
/// - **Responsiveness**: Immediate response to user input
///
/// # Integration Points
/// - **ButtonInput<KeyCode>**: Bevy's keyboard input system
/// - **EventWriter<ResetFogOfWar>**: Sends reset events to fog system
/// - **Logging**: Provides user feedback via info! macro
/// - **Reset System**: Triggers complete fog state reset
///
/// # Use Cases
/// - **Development**: Quick reset for testing different scenarios
/// - **User Interface**: Simple way to restart exploration
/// - **Demonstration**: Reset fog for repeated demos
/// - **Debugging**: Clear state for testing specific conditions
///
/// # Safety Considerations
/// - **Data Loss**: Reset operation is irreversible
/// - **User Intent**: Single key press prevents accidental resets
/// - **Logging**: Clear feedback about reset initiation
/// - **Event System**: Proper event handling ensures reliable reset
fn handle_reset_input(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut reset_events: EventWriter<ResetFogOfWar>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyR) {
        info!("Resetting fog of war...");
        reset_events.write(ResetFogOfWar);
    }
}

/// System that handles keyboard input for fog of war save and load operations.
/// 处理战争迷雾保存和加载操作的键盘输入系统
///
/// This system provides a comprehensive persistence interface with intelligent
/// format selection and automatic fallback mechanisms. It supports multiple
/// serialization formats with compression, automatically choosing the best
/// available option based on compiled features.
///
/// # Controls
/// - **P Key**: Save fog data with automatic format selection
/// - **L Key**: Load fog data with automatic format detection
///
/// # Save Operation (P Key)
/// When save is triggered:
/// 1. **Event Creation**: SaveFogOfWarRequest with texture data included
/// 2. **Format Selection**: Uses None for automatic format prioritization
/// 3. **Priority Order**: bincode → messagepack → json (best to fallback)
/// 4. **Compression**: Automatic compression when features available
/// 5. **User Feedback**: Log message confirms save initiation
///
/// # Load Operation (L Key)
/// Implements intelligent file detection and loading:
///
/// ## Format Priority Search
/// Attempts to load files in order of efficiency:
/// 1. **bincode.zst**: Binary + Zstd compression (best performance)
/// 2. **msgpack.lz4**: MessagePack + LZ4 compression (good performance)
/// 3. **bincode**: Binary format (compact)
/// 4. **msgpack**: MessagePack format (portable)
/// 5. **json**: JSON format (human readable, fallback)
///
/// ## File Detection Process
/// ```rust
/// for format in ["bincode.zst", "msgpack.lz4", "bincode", "msgpack", "json"] {
///     if file_exists(format!("fog_save.{}", format)) {
///         load_file_and_send_event();
///         break;
///     }
/// }
/// ```
///
/// ## Automatic Format Detection
/// - **Binary Reading**: Loads file as raw bytes
/// - **Format Detection**: Uses None for automatic content-based detection
/// - **Error Handling**: Continues to next format if current fails
/// - **User Feedback**: Success or failure logging
///
/// # Conditional Compilation
/// Format availability depends on cargo features:
/// - **format-bincode**: Enables binary serialization
/// - **format-messagepack**: Enables MessagePack serialization
/// - **compression-zstd**: Enables Zstd compression
/// - **compression-lz4**: Enables LZ4 compression
/// - **Default**: JSON always available as fallback
///
/// # Performance Characteristics
/// - **Input Polling**: Checks P and L keys every frame
/// - **File I/O**: Disk operations only when keys pressed
/// - **Format Detection**: Sequential file existence checks
/// - **Memory**: File content loaded into memory during load
/// - **CPU Cost**: Minimal until persistence operations triggered
///
/// # Error Handling
/// - **File Not Found**: Continues to next format in priority list
/// - **Read Failures**: Logs warning if no files found
/// - **Format Errors**: Handled by fog system during deserialization
/// - **User Feedback**: Clear success/failure messages
///
/// # Integration Points
/// - **SaveFogOfWarRequest**: Event for save operations
/// - **LoadFogOfWarRequest**: Event for load operations
/// - **File System**: Direct file I/O for format detection
/// - **Feature Flags**: Conditional compilation for formats
/// - **Logging**: Comprehensive user feedback
///
/// # File Organization
/// Expected file structure:
/// ```text
/// fog_save.bincode.zst   (highest priority)
/// fog_save.msgpack.lz4
/// fog_save.bincode
/// fog_save.msgpack
/// fog_save.json         (lowest priority, always available)
/// ```
fn handle_persistence_input(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut save_events: EventWriter<SaveFogOfWarRequest>,
    mut load_events: EventWriter<LoadFogOfWarRequest>,
    mut force_snapshot_events: EventWriter<ForceSnapshotCapturables>,
    _player_query: Query<&Player>,
) {
    // 保存雾效数据
    // Save fog data
    if keyboard_input.just_pressed(KeyCode::KeyP) {
        info!("Saving fog data");
        save_events.write(SaveFogOfWarRequest {
            include_texture_data: true,
            format: None, // Use default format (prioritizes bincode -> messagepack -> json)
        });
    }

    // 加载雾效数据
    // Load fog data
    if keyboard_input.just_pressed(KeyCode::KeyL) {
        // 尝试按优先级顺序加载不同格式的文件
        // Try loading different format files in priority order
        let format_priorities = [
            #[cfg(all(feature = "format-bincode", feature = "compression-zstd"))]
            "bincode.zst",
            #[cfg(all(feature = "format-messagepack", feature = "compression-lz4"))]
            "msgpack.lz4",
            #[cfg(feature = "format-bincode")]
            "bincode",
            #[cfg(feature = "format-messagepack")]
            "msgpack",
            "json",
        ];

        let mut loaded = false;
        for ext in format_priorities {
            let filename = format!("fog_save.{ext}");

            // 直接读取文件为字节数据
            // Read file as bytes directly
            match std::fs::read(&filename) {
                Ok(data) => {
                    info!("✅ Loaded fog data from '{}'", filename);
                    load_events.write(LoadFogOfWarRequest {
                        data,
                        format: None, // Auto-detect format from data content
                    });
                    loaded = true;
                    break;
                }
                Err(_) => {
                    // 文件不存在或加载失败，尝试下一个格式
                    // File doesn't exist or failed to load, try next format
                    continue;
                }
            }
        }

        if !loaded {
            warn!("⚠️ No save file found");
        }
    }

    // F12键 - 强制快照屏幕中的所有Capturable实体
    // F12 key - Force snapshot all Capturable entities on screen
    if keyboard_input.just_pressed(KeyCode::F12) {
        info!("Forcing snapshots for all on-screen Capturable entities...");
        force_snapshot_events.write(ForceSnapshotCapturables);
    }
}

/// System that processes fog of war save completion events and writes data to disk.
/// 处理战争迷雾保存完成事件并将数据写入磁盘的系统
///
/// This system handles the final stage of the save process by writing serialized
/// fog data to disk files. It receives save completion events from the fog system
/// and manages file creation, naming, and error handling.
///
/// # Event Processing
/// For each FogOfWarSaved event received:
/// 1. **Format Detection**: Determines file extension based on serialization format
/// 2. **File Writing**: Writes binary data directly to disk
/// 3. **Error Handling**: Comprehensive error logging and user feedback
/// 4. **Size Reporting**: Displays file size and chunk count information
///
/// # File Naming Convention
/// Files are named based on serialization format:
/// - **SerializationFormat::Json**: "fog_save.json"
/// - **SerializationFormat::MessagePack**: "fog_save.msgpack" (if feature enabled)
/// - **SerializationFormat::Bincode**: "fog_save.bincode" (if feature enabled)
///
/// # File Content
/// - **Binary Data**: Direct write of event.data bytes to disk
/// - **No Transcoding**: Uses serialized data exactly as provided
/// - **Compression**: Data may already be compressed by fog system
/// - **Format Preservation**: Maintains exact serialization format
///
/// # Success Feedback
/// When save succeeds:
/// ```text
/// ✅ Saved 47 chunks to 'fog_save.bincode' (234.5 KB) - Format: Bincode
/// ```
/// - **✅ Icon**: Visual success indicator
/// - **Chunk Count**: Number of chunks saved
/// - **File Name**: Exact filename created
/// - **File Size**: Human-readable file size
/// - **Format**: Serialization format used
///
/// # Error Feedback
/// When save fails:
/// ```text
/// ❌ Failed to save fog data to 'fog_save.json': Permission denied
/// ```
/// - **❌ Icon**: Visual error indicator
/// - **File Name**: Attempted filename
/// - **Error Details**: Specific OS error message
///
/// # Performance Characteristics
/// - **Event Frequency**: Only when save operations complete
/// - **I/O Operations**: Direct file write to disk
/// - **Memory**: Temporary access to serialized data
/// - **Error Handling**: Immediate feedback on write status
/// - **File Size**: Varies based on explored area and format
///
/// # Integration Points
/// - **FogOfWarSaved Event**: Receives completion events from fog system
/// - **File System**: Direct disk I/O operations
/// - **Size Utilities**: Uses get_file_size_info for human-readable sizes
/// - **Logging**: Provides user feedback via info! and error! macros
///
/// # File System Considerations
/// - **Permissions**: Requires write access to current directory
/// - **Disk Space**: File size depends on world exploration extent
/// - **Overwriting**: Replaces existing files with same names
/// - **Error Recovery**: Logs errors but doesn't retry or rollback
///
/// # Data Integrity
/// - **Direct Write**: Preserves exact serialized data
/// - **No Modification**: System doesn't alter fog system output
/// - **Atomic Operations**: File write is atomic (success or failure)
/// - **Error Detection**: Immediate feedback on write failures
fn handle_saved_event(mut events: EventReader<FogOfWarSaved>) {
    for event in events.read() {
        // 直接使用序列化后的二进制数据
        // Use the serialized binary data directly
        let filename = match event.format {
            #[cfg(feature = "format-json")]
            SerializationFormat::Json => "fog_save.json",
            #[cfg(feature = "format-messagepack")]
            SerializationFormat::MessagePack => "fog_save.msgpack",
            #[cfg(feature = "format-bincode")]
            SerializationFormat::Bincode => "fog_save.bincode",
        };

        match std::fs::write(filename, &event.data) {
            Ok(_) => {
                if let Ok(size) = get_file_size_info(filename) {
                    info!(
                        "✅ Saved {} chunks to '{}' ({}) - Format: {:?}",
                        event.chunk_count, filename, size, event.format
                    );
                }
            }
            Err(e) => {
                error!("❌ Failed to save fog data to '{}': {}", filename, e);
            }
        }
    }
}

/// System that processes fog of war load completion events and provides user feedback.
/// 处理战争迷雾加载完成事件并提供用户反馈的系统
///
/// This system handles the completion of fog data loading operations by
/// processing load result events and providing comprehensive feedback
/// to the user about the success or issues encountered during loading.
///
/// # Event Processing
/// For each FogOfWarLoaded event received:
/// 1. **Success Logging**: Reports successful load with chunk count
/// 2. **Warning Processing**: Displays any warnings encountered during load
/// 3. **User Feedback**: Provides clear status information
/// 4. **Debug Information**: Helps identify potential data issues
///
/// # Success Feedback
/// When load completes successfully:
/// ```text
/// Successfully loaded 47 chunks
/// ```
/// - **Chunk Count**: Number of chunks restored from saved data
/// - **Confirmation**: Clear indication that operation completed
/// - **Data Integrity**: Implies successful data restoration
///
/// # Warning Handling
/// If warnings occurred during loading:
/// ```text
/// Successfully loaded 47 chunks
/// Load warnings:
///   - Chunk (2, 3) has invalid texture data, using defaults
///   - Missing fog layer allocation for chunk (-1, 5)
/// ```
/// - **Warning Section**: Clearly separated warning information
/// - **Specific Issues**: Detailed description of each warning
/// - **Impact Assessment**: Helps user understand data quality
///
/// # Warning Categories
/// Common warnings include:
/// - **Invalid Texture Data**: Corrupted or incompatible texture information
/// - **Missing Allocations**: GPU layer allocation failures
/// - **Format Compatibility**: Version or feature compatibility issues
/// - **Partial Data**: Some chunks missing or incomplete
/// - **Resource Constraints**: GPU memory limitations during restore
///
/// # Performance Characteristics
/// - **Event Frequency**: Only when load operations complete
/// - **Processing Cost**: Minimal string formatting and logging
/// - **Memory**: Temporary access to warning strings
/// - **User Impact**: Immediate feedback on load status
///
/// # Integration Points
/// - **FogOfWarLoaded Event**: Receives completion events from fog system
/// - **Logging System**: Uses info! and warn! macros for output
/// - **User Interface**: Provides console feedback for load operations
/// - **Debug Support**: Warning details help diagnose load issues
///
/// # User Experience
/// - **Clear Feedback**: Immediate confirmation of load success
/// - **Issue Transparency**: Warnings help user understand data state
/// - **No Action Required**: System handles warnings automatically
/// - **Debug Support**: Warning details available for troubleshooting
///
/// # Data Quality Assessment
/// The warning system helps users understand:
/// - **Save File Integrity**: Whether saved data is fully intact
/// - **Compatibility Issues**: Version or feature mismatches
/// - **Resource Limitations**: Current system constraints
/// - **Expected Behavior**: What to expect after load with warnings
fn handle_loaded_event(mut events: EventReader<FogOfWarLoaded>) {
    for event in events.read() {
        info!("Successfully loaded {} chunks", event.chunk_count);

        if !event.warnings.is_empty() {
            warn!("Load warnings:");
            for warning in &event.warnings {
                warn!("  - {}", warning);
            }
        }
    }
}
