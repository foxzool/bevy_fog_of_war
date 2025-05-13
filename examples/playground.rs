use bevy::diagnostic::FrameCount;
use bevy::{
    color::palettes::css::{GOLD, RED},
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
// use bevy_inspector_egui::bevy_egui::EguiPlugin;
// use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_fog_of_war::prelude::*;

/// 移动目标位置资源
/// Movement target position resource
#[derive(Resource, Default)]
struct TargetPosition(Option<Vec3>);

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
        // .add_plugins(EguiPlugin {
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

/// 水平来回移动的 Sprite 标记
/// Marker for the horizontally moving sprite
#[derive(Component)]
struct HorizontalMover {
    direction: f32, // 1.0 for right, -1.0 for left
}

#[derive(Component)]
struct FogOfWarCamera;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
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
            font: font_handle.clone().into(),
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
        Transform::from_translation(Vec3::new(-10.0, -50.0, 0.0)),
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
        Transform::from_translation(Vec3::new(-200.0, -0.0, 0.0)),
        Snapshottable,
        // RenderLayers::layer(SNAPSHOT_RENDER_LAYER)
    ));

    // 生成可移动的视野提供者
    // Spawn movable vision provider
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

    // 颜色渐变条作为参考，并添加视野提供者组件到部分方块
    // Color gradient bar as reference, and add vision provider to some blocks
    for i in 0..10 {
        let position = Vec3::new(-500.0 + i as f32 * 100.0, 200.0, 0.0);
        let color = Color::hsl(i as f32 * 36.0, 0.8, 0.5);

        // 只给偶数索引的方块添加视野提供者组件
        // Only add vision provider to blocks with even indices
        let mut entity_commands = commands.spawn((
            Sprite {
                color,
                custom_size: Some(Vec2::new(80.0, 80.0)),
                ..default()
            },
            Transform::from_translation(position),
        ));

        // 为偶数索引的方块添加视野提供者
        // Add vision provider to blocks with even indices
        if i % 2 == 0 {
            entity_commands.insert(VisionSource {
                range: 30.0 + (i as f32 * 15.0),
                enabled: true,
                shape: VisionShape::Cone,
                direction: (i as f32 * 25.0),
                angle: std::f32::consts::FRAC_PI_2,
                intensity: 1.0,
                transition_ratio: 0.2,
            });
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
            "Fog Status: {}\nPress F to toggle\nPress Up/Down to adjust Alpha: {:.0}%",
            status, alpha_percentage
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

    // 创建颜色动画标题文本
    // Create color animated title text
    commands.spawn((
        Text::new("Fog of War System"),
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
