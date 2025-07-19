use bevy::diagnostic::FrameCount;
use bevy::{
    color::palettes::css::{GOLD, RED},
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_fog_of_war::prelude::*;
use std::fs;

/// 移动目标位置资源
/// Movement target position resource
#[derive(Resource, Default)]
struct TargetPosition(Option<Vec3>);

/// 玩家标记组件
/// Player marker component
#[derive(Component)]
struct Player {
    character_id: String,
}

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

#[derive(Default, Reflect, GizmoConfigGroup)]
struct MyRoundGizmos {}

// 迷雾材质组件的标记，用于在 update_fog_settings 中查找并移除
// Marker component for FogMaterial, used in update_fog_settings to find and remove
#[derive(Component)]
struct FogMaterialComponent;

/// 帧率文本组件标记
/// FPS text component marker
#[derive(Component)]
struct FpsText;

/// 迷雾设置文本组件标记
/// Fog settings text component marker
#[derive(Component)]
struct FogSettingsText;

/// 颜色动画文本组件标记
/// Color animation text component marker
#[derive(Component)]
struct ColorAnimatedText;

/// 计数文本组件标记
/// Count text component marker
#[derive(Component)]
struct CountText;

/// 可移动视野提供者标记
/// Movable vision provider marker
#[derive(Component)]
struct MovableVision;

#[derive(Component)]
struct RotationAble;

/// 水平来回移动的 Sprite 标记
/// Marker for the horizontally moving sprite
#[derive(Component)]
struct HorizontalMover {
    direction: f32, // 1.0 for right, -1.0 for left
}

const X_EXTENT: f32 = 900.;

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
        Player {
            character_id: "player_1".to_string(),
        },
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

// 相机移动系统
// Camera movement system
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

// 更新迷雾设置系统
// Update fog settings system
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

// 设置 UI 系统
// Setup UI system
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
             P - Save fog data (multiple formats)\n\
             L - Load fog data (auto-detects format)\n\
             1-3 - Load different character saves\n\
             Supports: JSON, MessagePack, bincode + compression",
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

// 更新 FPS 文本系统
// Update FPS text system
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

fn update_count_text(mut query: Query<&mut Text2d, With<CountText>>, frame_count: Res<FrameCount>) {
    for mut text in &mut query {
        text.0 = format!("Count: {}", frame_count.0);
    }
}

// 可移动视野控制系统
// Movable vision control system
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

// 新增：水平移动系统
// New: Horizontal movement system
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

/// Rotates entities with the `RotationAble` component.
/// 旋转带有 `RotationAble` 组件的实体。
fn rotate_entities_system(time: Res<Time>, mut query: Query<&mut Transform, With<RotationAble>>) {
    for mut transform in query.iter_mut() {
        transform.rotate_z(std::f32::consts::FRAC_PI_2 * time.delta_secs()); // 90 degrees per second / 每秒旋转90度
    }
}

/// 监听雾效重置事件的系统
/// System that listens to fog reset events
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

/// 在屏幕上绘制区块边界和状态的调试信息
/// System to draw chunk boundaries and status for debugging
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

// 处理重置输入系统
// Handle reset input system
fn handle_reset_input(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut reset_events: EventWriter<ResetFogOfWar>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyR) {
        info!("Resetting fog of war...");
        reset_events.write(ResetFogOfWar);
    }
}

// 处理持久化输入系统
// Handle persistence input system
fn handle_persistence_input(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut save_events: EventWriter<SaveFogOfWarRequest>,
    mut load_events: EventWriter<LoadFogOfWarRequest>,
    player_query: Query<&Player>,
) {
    // 保存当前玩家的雾效数据
    // Save current player's fog data
    if keyboard_input.just_pressed(KeyCode::KeyP) {
        if let Ok(player) = player_query.single() {
            info!("Saving fog data for character: {}", player.character_id);
            save_events.write(SaveFogOfWarRequest {
                character_id: player.character_id.clone(),
                include_texture_data: true,
            });
        }
    }

    // 加载当前玩家的雾效数据
    // Load current player's fog data
    if keyboard_input.just_pressed(KeyCode::KeyL) {
        if let Ok(player) = player_query.single() {
            // 尝试加载不同格式的文件（按优先级排序：二进制压缩 > 二进制 > JSON压缩 > JSON）
            // Try loading different file formats (priority: binary compressed > binary > JSON compressed > JSON)
            let format_priorities = vec![
                // 最高优先级：二进制压缩格式
                // Highest priority: binary compressed formats
                #[cfg(all(feature = "format-bincode", feature = "compression-zstd"))]
                "bincode.zst",
                #[cfg(all(feature = "format-messagepack", feature = "compression-zstd"))]
                "msgpack.zst",
                #[cfg(all(feature = "format-bincode", feature = "compression-lz4"))]
                "bincode.lz4",
                #[cfg(all(feature = "format-messagepack", feature = "compression-lz4"))]
                "msgpack.lz4",
                #[cfg(all(feature = "format-bincode", feature = "compression-gzip"))]
                "bincode.gz",
                #[cfg(all(feature = "format-messagepack", feature = "compression-gzip"))]
                "msgpack.gz",
                
                // 二进制格式
                // Binary formats
                #[cfg(feature = "format-bincode")]
                "bincode",
                #[cfg(feature = "format-messagepack")]
                "msgpack",
                
                // JSON压缩格式
                // JSON compressed formats
                #[cfg(feature = "compression-zstd")]
                "json.zst",
                #[cfg(feature = "compression-lz4")]
                "json.lz4", 
                #[cfg(feature = "compression-gzip")]
                "json.gz",
                
                // 基础JSON格式
                // Basic JSON format
                "json",
            ];
            
            let mut loaded = false;
            for ext in format_priorities {
                let filename = format!("fog_save_{}.{}", player.character_id, ext);
                if std::path::Path::new(&filename).exists() {
                    // 对于二进制格式，使用新的load_data_from_file函数直接加载
                    // For binary formats, use the new load_data_from_file function to load directly
                    let result = if ext.contains("msgpack") || ext.contains("bincode") {
                        match load_data_from_file::<FogOfWarSaveData>(&filename, None) {
                            Ok(save_data) => {
                                match serde_json::to_string(&save_data) {
                                    Ok(json_data) => Ok(json_data),
                                    Err(e) => Err(PersistenceError::SerializationFailed(e.to_string())),
                                }
                            }
                            Err(e) => Err(e),
                        }
                    } else {
                        load_from_file(&filename, None)
                    };
                    
                    match result {
                        Ok(data) => {
                            info!("Loading fog data from '{}' for character: {} (Format: auto-detected)", 
                                  filename, player.character_id);
                            load_events.write(LoadFogOfWarRequest {
                                character_id: player.character_id.clone(),
                                data,
                            });
                            loaded = true;
                            break;
                        }
                        Err(e) => {
                            error!("Failed to read save file '{}': {}", filename, e);
                        }
                    }
                }
            }
            
            if !loaded {
                warn!("No save file found for character: {}", player.character_id);
            }
        }
    }

    // 加载不同角色的存档
    // Load different character saves
    for (key, character_id) in [
        (KeyCode::Digit1, "player_1"),
        (KeyCode::Digit2, "player_2"),
        (KeyCode::Digit3, "player_3"),
    ] {
        if keyboard_input.just_pressed(key) {
            let filename = format!("fog_save_{}.json", character_id);
            match fs::read_to_string(&filename) {
                Ok(data) => {
                    info!("Loading fog data for character: {}", character_id);
                    load_events.write(LoadFogOfWarRequest {
                        character_id: character_id.to_string(),
                        data,
                    });
                }
                Err(_) => {
                    warn!("No save file found for character: {}", character_id);
                }
            }
        }
    }
}

// 处理保存完成事件
// Handle saved event
fn handle_saved_event(mut events: EventReader<FogOfWarSaved>) {
    for event in events.read() {
        // 使用便利函数保存到文件
        // Use utility function to save to file
        
        // 尝试不同的序列化格式和压缩组合
        // Try different serialization formats and compression combinations
        let formats = vec![
            (FileFormat::Json, "json"),
            
            // 压缩的JSON格式
            // Compressed JSON formats
            #[cfg(feature = "compression-gzip")]
            (FileFormat::JsonGzip, "json.gz"),
            #[cfg(feature = "compression-lz4")]
            (FileFormat::JsonLz4, "json.lz4"),
            #[cfg(feature = "compression-zstd")]
            (FileFormat::JsonZstd, "json.zst"),
            
            // MessagePack格式
            // MessagePack formats
            #[cfg(feature = "format-messagepack")]
            (FileFormat::MessagePack, "msgpack"),
            #[cfg(all(feature = "format-messagepack", feature = "compression-gzip"))]
            (FileFormat::MessagePackGzip, "msgpack.gz"),
            #[cfg(all(feature = "format-messagepack", feature = "compression-lz4"))]
            (FileFormat::MessagePackLz4, "msgpack.lz4"),
            #[cfg(all(feature = "format-messagepack", feature = "compression-zstd"))]
            (FileFormat::MessagePackZstd, "msgpack.zst"),
            
            // bincode格式
            // bincode formats
            #[cfg(feature = "format-bincode")]
            (FileFormat::Bincode, "bincode"),
            #[cfg(all(feature = "format-bincode", feature = "compression-gzip"))]
            (FileFormat::BincodeGzip, "bincode.gz"),
            #[cfg(all(feature = "format-bincode", feature = "compression-lz4"))]
            (FileFormat::BincodeLz4, "bincode.lz4"),
            #[cfg(all(feature = "format-bincode", feature = "compression-zstd"))]
            (FileFormat::BincodeZstd, "bincode.zst"),
        ];
        
        // 首先将JSON字符串反序列化为结构体
        // First deserialize JSON string to struct
        let save_data: Result<FogOfWarSaveData, _> = serde_json::from_str(&event.data);
        
        match save_data {
            Ok(data) => {
                for (format, ext) in formats {
                    let filename = format!("fog_save_{}.{}", event.character_id, ext);
                    
                    match save_data_to_file(&data, &filename, format) {
                        Ok(_) => {
                            if let Ok(size) = get_file_size_info(&filename) {
                                info!(
                                    "Saved {} chunks to '{}' ({}) - Format: {:?}",
                                    event.chunk_count, filename, size, format
                                );
                            }
                        }
                        Err(e) => {
                            error!("Failed to save as {}: {}", ext, e);
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to parse save data: {}", e);
            }
        }
    }
}

// 处理加载完成事件
// Handle loaded event
fn handle_loaded_event(mut events: EventReader<FogOfWarLoaded>) {
    for event in events.read() {
        info!(
            "Successfully loaded {} chunks for character '{}'",
            event.chunk_count, event.character_id
        );

        if !event.warnings.is_empty() {
            warn!("Load warnings:");
            for warning in &event.warnings {
                warn!("  - {}", warning);
            }
        }
    }
}
