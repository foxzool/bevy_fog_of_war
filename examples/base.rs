use bevy::prelude::*;
use bevy_fog_of_war::{FogOfWarPlugin, FogOfWarConfig, setup_fog_of_war, FogSettings};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Fog of War Example".into(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(FogOfWarConfig {
            chunk_size: 256.0,
            view_range: 5,
            debug_draw: true,
        })
        .add_plugins(FogOfWarPlugin)
        .insert_resource(FogSettings {
            color: Color::srgba(0.1, 0.1, 0.2, 1.0),
            density: 0.003,
            fog_range: 2000.0,
            max_intensity: 0.9,
        })
        .add_systems(Startup, (setup, setup_fog_of_war))
        .add_systems(Update, (camera_movement, update_fog_settings))
        .run();
}

#[derive(Component)]
struct MainCamera;

fn setup(mut commands: Commands) {
    // 生成相机
    // Spawn camera
    commands.spawn((
        Camera2d,
        MainCamera,
    ));

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
    // 调整迷雾密度
    // Adjust fog density
    if keyboard.pressed(KeyCode::KeyZ) {
        fog_settings.density = (fog_settings.density - 0.001 * time.delta_secs()).max(0.0001);
        println!("迷雾密度 / Fog density: {}", fog_settings.density);
    }
    if keyboard.pressed(KeyCode::KeyX) {
        fog_settings.density = (fog_settings.density + 0.001 * time.delta_secs()).min(0.01);
        println!("迷雾密度 / Fog density: {}", fog_settings.density);
    }
    
    // 调整迷雾最大强度
    // Adjust maximum fog intensity
    if keyboard.pressed(KeyCode::KeyC) {
        fog_settings.max_intensity = (fog_settings.max_intensity - 0.1 * time.delta_secs()).max(0.1);
        println!("迷雾最大强度 / Max fog intensity: {}", fog_settings.max_intensity);
    }
    if keyboard.pressed(KeyCode::KeyV) {
        fog_settings.max_intensity = (fog_settings.max_intensity + 0.1 * time.delta_secs()).min(1.0);
        println!("迷雾最大强度 / Max fog intensity: {}", fog_settings.max_intensity);
    }
}