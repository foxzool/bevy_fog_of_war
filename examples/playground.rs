//! Interactive Fog of War Playground Example
//! 交互式战争迷雾游乐场示例

// Allow collapsible_if for stable Rust compatibility
#![allow(clippy::collapsible_if)]
//!
//! Demonstrates all features of the bevy_fog_of_war plugin in an interactive playground.
//!
//! ## Controls
//! - **WASD**: Move camera
//! - **Arrow Keys**: Move player
//! - **Mouse**: Click to move player
//! - **F**: Toggle fog on/off
//! - **R**: Reset fog
//! - **PageUp/Down**: Adjust fog transparency
//! - **P/L**: Save/Load fog data
//! - **F12**: Force snapshot of all Capturable entities
//!
//! ## Running
//! ```bash
//! cargo run --example playground
//! ```

use bevy::diagnostic::FrameCount;
use bevy::{
    color::palettes::css::{GOLD, RED},
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_fog_of_war::prelude::*;

/// Target position for click-to-move functionality.
#[derive(Resource, Default)]
struct TargetPosition(Option<Vec3>);

/// Marks the player entity.
#[derive(Component)]
struct Player;

/// Main entry point for the fog of war playground example.
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

/// Marks cameras that need fog material management.
#[derive(Component)]
struct FogMaterialComponent;

/// Marks UI text elements that display FPS.
#[derive(Component)]
struct FpsText;

/// Marks UI text elements that display fog settings and statistics.
#[derive(Component)]
struct FogSettingsText;

/// Marks UI text elements for color animation (not yet implemented).
#[derive(Component)]
struct ColorAnimatedText;

/// Marks UI text elements that display frame count.
#[derive(Component)]
struct CountText;

/// Marks vision sources that can be controlled by user input (arrows/mouse).
#[derive(Component)]
struct MovableVision;

/// Marks entities that should rotate continuously around Z-axis.
#[derive(Component)]
struct RotationAble;

/// Component for entities that move horizontally back and forth.
#[derive(Component)]
struct HorizontalMover {
    direction: f32, // 1.0 for right, -1.0 for left
}

/// Horizontal extent for distributing geometric shapes across the scene.
const X_EXTENT: f32 = 900.;

/// Sets up the initial scene with camera, entities, and vision sources.
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

/// Handles camera movement via WASD keys.
fn camera_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut camera_query: Query<&mut Transform, With<FogOfWarCamera>>,
    _window_query: Query<&Window>,
) {
    if let Ok(mut camera_transform) = camera_query.single_mut() {
        let mut direction = Vec3::ZERO;
        let speed = 500.0;

        // WASD keys control movement
        if keyboard.pressed(KeyCode::KeyW) {
            direction.y += 1.0;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            direction.y -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            direction.x -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            direction.x += 1.0;
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

/// Updates fog settings based on keyboard input.
/// F: toggle fog, PageUp/Down: adjust transparency.
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
/// Creates UI elements for FPS display, fog settings, controls, and title text.
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

/// Updates the FPS display text with current frame rate.
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

/// Updates count display text with current frame number.
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

/// Handles player movement via arrow keys and mouse click-to-move.
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
        let speed = 200.0;
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
    let speed = 150.0;
    let left_bound = -450.0;
    let right_bound = 450.0;

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

/// Continuously rotates entities marked with RotationAble around Z-axis.
fn rotate_entities_system(time: Res<Time>, mut query: Query<&mut Transform, With<RotationAble>>) {
    for mut transform in query.iter_mut() {
        transform.rotate_z(std::f32::consts::FRAC_PI_2 * time.delta_secs()); // 90 degrees per second / 每秒旋转90度
    }
}

/// Logs fog reset operation results for debugging and user feedback.
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

/// Draws chunk boundaries and debug info when fog is disabled.
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

/// Handles P key (save) and L key (load) for fog persistence with format auto-detection.
fn handle_persistence_input(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut save_events: EventWriter<SaveFogOfWarRequest>,
    mut load_events: EventWriter<LoadFogOfWarRequest>,
    capturable_entities: Query<Entity, With<Capturable>>,
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

    // F12键 - 强制快照所有Capturable实体
    // F12 key - Force snapshot all Capturable entities
    if keyboard_input.just_pressed(KeyCode::F12) {
        info!("Triggering snapshots for all Capturable entities...");
        for entity in capturable_entities.iter() {
            commands.entity(entity).insert(ForceSnapshotCapturables);
        }
    }
}

/// Writes fog save completion events to disk files with appropriate extensions.
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

/// Logs fog load completion results and any warnings.
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
