use bevy::color::palettes::basic;
use bevy::color::palettes::css::{GOLD, RED};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_fog_of_war::{
    setup_fog_of_war, FogCameraMarker, FogOfWarConfig, FogOfWarPlugin, FogSettings,
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Fog of War Example".into(),
                    resolution: (1280.0, 720.0).into(),
                    ..default()
                }),
                ..default()
            }),
            FrameTimeDiagnosticsPlugin
        ))
        .insert_resource(FogOfWarConfig {
            chunk_size: 256.0,
            view_range: 5,
            debug_draw: true,
        })
        .add_plugins(FogOfWarPlugin)
        .insert_resource(FogSettings {
            color: Color::srgba(1.0, 0.0, 0.0, 1.0),
            density: 0.2,
            fog_range: 1000.0,
            max_intensity: 1.0,
            // 相机周围的透明区域半径
            // Clear radius around camera
            clear_radius: 2.0,
            // 边缘过渡效果宽度
            // Edge falloff width
            clear_falloff: 1.0,
        })
        .add_systems(Startup, (setup, setup_fog_of_war, setup_ui))
        .add_systems(Update, (camera_movement, update_fog_settings, update_fps_text, update_fog_settings_text, text_color_animation))
        .run();
}

#[derive(Component)]
struct MainCamera;

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

fn setup(mut commands: Commands, mut fog_settings: ResMut<FogSettings>) {
    // 配置迷雾设置
    // Configure fog settings
    *fog_settings = FogSettings {
        // 使用深蓝色迷雾，透明度设置为 0.7
        // Use deep blue fog with 0.7 alpha
        color: Color::rgba(0.1, 0.2, 0.4, 0.7),
        // 中等密度
        // Medium density
        density: 0.6,
        // 迷雾范围
        // Fog range
        fog_range: 1.5,
        // 最大强度
        // Maximum intensity
        max_intensity: 0.85,
        // 相机周围的透明区域半径
        // Clear radius around camera
        clear_radius: 0.3,
        // 边缘过渡效果宽度
        // Edge falloff width
        clear_falloff: 0.1,
    };
    
    // 生成相机
    // Spawn camera
    commands.spawn((Camera2d, FogCameraMarker, MainCamera));
    
    // 生成一个红色方块来测试基本渲染功能
    // Spawn a red square to test basic rendering functionality
    commands.spawn(
        SpriteBundle {
            sprite: Sprite {
                color: basic::RED.into(),
                custom_size: Some(Vec2::new(100.0, 100.0)),
                ..default()
            },
            transform: Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
            ..default()
        }
    );

    // 颜色渐变条作为参考
    // Color gradient bar as reference
    for i in 0..10 {
        let position = Vec3::new(-500.0 + i as f32 * 100.0, 200.0, 0.0);
        let color = Color::hsl(i as f32 * 36.0, 0.8, 0.5);
        commands.spawn(
            SpriteBundle {
                sprite: Sprite {
                    color,
                    custom_size: Some(Vec2::new(80.0, 80.0)),
                    ..default()
                },
                transform: Transform::from_translation(position),
                ..default()
            }
        );
    }
}

// 相机移动系统
// Camera movement system
fn camera_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut camera_query: Query<&mut Transform, With<MainCamera>>,
) {
    if let Ok(mut camera_transform) = camera_query.get_single_mut() {
        let mut direction = Vec3::ZERO;
        let speed = 500.0; // 移动速度 / Movement speed

        if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
            direction.y += 1.0;
        }
        if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
            direction.y -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
            direction.x -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
            direction.x += 1.0;
        }

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
    mut fog_settings: ResMut<FogSettings>,
) {
    let delta = time.delta_secs();
    let mut changed = false;
    
    // 调整迷雾密度
    // Adjust fog density
    if keyboard.pressed(KeyCode::KeyZ) {
        fog_settings.density = (fog_settings.density - 0.1 * delta).max(0.1);
        changed = true;
    }
    if keyboard.pressed(KeyCode::KeyX) {
        fog_settings.density = (fog_settings.density + 0.1 * delta).min(1.0);
        changed = true;
    }

    // 调整迷雾范围
    // Adjust fog range
    if keyboard.pressed(KeyCode::KeyC) {
        fog_settings.fog_range = (fog_settings.fog_range - 0.2 * delta).max(0.5);
        changed = true;
    }
    if keyboard.pressed(KeyCode::KeyV) {
        fog_settings.fog_range = (fog_settings.fog_range + 0.2 * delta).min(3.0);
        changed = true;
    }

    // 调整迷雾最大强度
    // Adjust maximum fog intensity
    if keyboard.pressed(KeyCode::KeyB) {
        fog_settings.max_intensity = (fog_settings.max_intensity - 0.1 * delta).max(0.1);
        changed = true;
    }
    if keyboard.pressed(KeyCode::KeyN) {
        fog_settings.max_intensity = (fog_settings.max_intensity + 0.1 * delta).min(1.0);
        changed = true;
    }
    
    // 切换迷雾颜色
    // Toggle fog color
    if keyboard.just_pressed(KeyCode::Digit1) {
        // 蓝色迷雾 / Blue fog
        fog_settings.color = Color::rgba(0.1, 0.2, 0.4, 0.7);
        changed = true;
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        // 红色迷雾 / Red fog
        fog_settings.color = Color::rgba(0.4, 0.1, 0.1, 0.7);
        changed = true;
    }
    if keyboard.just_pressed(KeyCode::Digit3) {
        // 绿色迷雾 / Green fog
        fog_settings.color = Color::rgba(0.1, 0.3, 0.1, 0.7);
        changed = true;
    }
    if keyboard.just_pressed(KeyCode::Digit4) {
        // 紫色迷雾 / Purple fog
        fog_settings.color = Color::rgba(0.3, 0.1, 0.3, 0.7);
        changed = true;
    }
    
    // 调整相机周围的透明区域半径
    // Adjust clear radius around camera
    if keyboard.pressed(KeyCode::Digit5) {
        fog_settings.clear_radius = (fog_settings.clear_radius - 0.1 * delta).max(0.0);
        changed = true;
    }
    if keyboard.pressed(KeyCode::Digit6) {
        fog_settings.clear_radius = (fog_settings.clear_radius + 0.1 * delta).min(1.0);
        changed = true;
    }
    
    // 调整边缘过渡效果宽度
    // Adjust edge falloff width
    if keyboard.pressed(KeyCode::Digit7) {
        fog_settings.clear_falloff = (fog_settings.clear_falloff - 0.1 * delta).max(0.01);
        changed = true;
    }
    if keyboard.pressed(KeyCode::Digit8) {
        fog_settings.clear_falloff = (fog_settings.clear_falloff + 0.1 * delta).min(0.5);
        changed = true;
    }
    
    // 如果设置发生变化，显示当前设置
    // If settings changed, display current settings
    if changed {
        println!(
            "迷雾设置 / Fog Settings: 颜色/Color: {:?}, 密度/Density: {:.2}, 范围/Range: {:.2}, 最大强度/Max: {:.2}, 透明半径/Clear: {:.2}, 过渡/Falloff: {:.2}",
            fog_settings.color, fog_settings.density, fog_settings.fog_range, fog_settings.max_intensity, fog_settings.clear_radius, fog_settings.clear_falloff
        );
    }
    if keyboard.pressed(KeyCode::KeyV) {
        fog_settings.max_intensity =
            (fog_settings.max_intensity + 0.1 * time.delta_secs()).min(1.0);
        println!(
            "迷雾最大强度 / Max fog intensity: {}",
            fog_settings.max_intensity
        );
    }
}

/// 设置 UI 系统
/// Setup UI system
fn setup_ui(mut commands: Commands, asset_server: Res<AssetServer>) {

    
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
        ))
        .with_child((
            TextSpan::default(),
            TextFont {
                font_size: 24.0,
                ..default()
            },
            TextColor(GOLD.into()),
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
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(20.0),
            right: Val::Px(20.0),
            ..default()
        },
        ColorAnimatedText,
    ));
}

/// 更新 FPS 文本系统
/// Update FPS text system
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

/// 更新迷雾设置文本系统
/// Update fog settings text system
fn update_fog_settings_text(
    fog_settings: Res<FogSettings>,
    mut query: Query<&mut Text, With<FogSettingsText>>,
) {
    for mut text in &mut query {
        // 格式化颜色显示
        // Format color display
        let color_text = format!(
            "R: {:.2}, G: {:.2}, B: {:.2}, A: {:.2}",
            fog_settings.color.to_linear().red,
            fog_settings.color.to_linear().green,
            fog_settings.color.to_linear().blue,
            fog_settings.color.to_linear().alpha
        );
        
        // 更新设置文本
        // Update settings text
        **text = format!(
            " Color: {}\n Density: {:.2}\n Range: {:.2}\n Max: {:.2}\n Clear: {:.2}\n Falloff: {:.2}",
            color_text,
            fog_settings.density,
            fog_settings.fog_range,
            fog_settings.max_intensity,
            fog_settings.clear_radius,
            fog_settings.clear_falloff
        );
    }
}

/// 文本颜色动画系统
/// Text color animation system
fn text_color_animation(time: Res<Time>, mut query: Query<&mut TextColor, With<ColorAnimatedText>>) {
    for mut text_color in &mut query {
        let seconds = time.elapsed_secs();
        
        // 更新颜色动画文本的颜色
        // Update the color of the animated text
        text_color.0 = Color::hsl(
            (seconds * 20.0) % 360.0,  // 色相随时间变化 / Hue changes with time
            0.7,                       // 饱和度 / Saturation
            0.7,                       // 亮度 / Lightness
        );
    }
}
