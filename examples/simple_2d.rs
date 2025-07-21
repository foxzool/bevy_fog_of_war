use bevy::prelude::*;
use bevy_fog_of_war::prelude::{
    Capturable, FogMapSettings, FogOfWarCamera, FogOfWarLoaded, FogOfWarPlugin, FogOfWarSaved,
    FogResetFailed, FogResetSuccess, ForceSnapshotCapturables, LoadFogOfWarRequest,
    SaveFogOfWarRequest, SerializationFormat, VisionSource,
};

fn main() {
    // Controls:
    // WASD - Move camera
    // F5 - Save fog data (demonstrates multiple serialization formats)
    // F9 - Load fog data (auto-detects format)
    // F12 - Force snapshot all Capturable entities on screen
    //
    // This example demonstrates the different serialization formats:
    // - JSON (human-readable, larger)
    // - MessagePack (binary, compact) - requires 'format-messagepack' feature
    // - bincode (Rust-native, fastest) - requires 'format-bincode' feature

    App::new()
        .insert_resource(ClearColor(Color::WHITE))
        .init_gizmo_group::<MyRoundGizmos>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Simple 2d Fog of War - Serialization Demo".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(FogOfWarPlugin)
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                draw_gizmos,
                camera_movement,
                handle_fog_reset_events,
                handle_save_load_input,
                handle_saved_event,
                handle_loaded_event,
            ),
        )
        .run();
}

#[derive(Default, Reflect, GizmoConfigGroup)]
struct MyRoundGizmos {}
const X_EXTENT: f32 = 900.;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut fog_map_settings: ResMut<FogMapSettings>,
) {
    // spawn camera with circle vision
    commands.spawn((Camera2d, FogOfWarCamera, VisionSource::circle(100.0)));

    // fog map settings
    *fog_map_settings = FogMapSettings {
        enabled: true,
        fog_color_unexplored: Color::BLACK,
        fog_color_explored: bevy::color::palettes::basic::GRAY.into(),
        vision_clear_color: Color::NONE,
        ..default()
    };

    // spawn other shapes vision sources on top screen
    commands.spawn((
        VisionSource::square(100.0),
        Transform::from_xyz(-300.0, 200.0, 0.0),
    ));
    commands.spawn((
        VisionSource::cone(100.0, 0.0, std::f32::consts::FRAC_PI_2),
        Transform::from_xyz(0.0, 200.0, 0.0),
    ));
    commands.spawn((
        VisionSource::circle(100.0),
        Transform::from_xyz(300.0, 200.0, 0.0),
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

        commands
            .spawn((
                Mesh2d(shape),
                MeshMaterial2d(materials.add(color)),
                Transform::from_xyz(
                    // Distribute shapes from -X_EXTENT/2 to +X_EXTENT/2.
                    -X_EXTENT / 2. + i as f32 / (num_shapes - 1) as f32 * X_EXTENT,
                    0.0,
                    0.0,
                ),
            ))
            .insert_if(Capturable, || i.is_multiple_of(2));
    }
}

fn draw_gizmos(mut gizmos: Gizmos) {
    gizmos
        .grid_2d(
            Isometry2d::IDENTITY,
            UVec2::new(16, 9),
            Vec2::new(80., 80.),
            // Dark gray
            LinearRgba::gray(0.05),
        )
        .outer_edges();
}

// move camera with WASD
fn camera_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut camera_query: Query<&mut Transform, With<FogOfWarCamera>>,
) {
    if let Ok(mut camera_transform) = camera_query.single_mut() {
        let mut direction = Vec3::ZERO;
        let speed = 500.0; //  Movement speed

        if keyboard.pressed(KeyCode::KeyW) {
            direction.y += 1.0; // Move up
        }
        if keyboard.pressed(KeyCode::KeyS) {
            direction.y -= 1.0; // Move down
        }
        if keyboard.pressed(KeyCode::KeyA) {
            direction.x -= 1.0; // Move left
        }
        if keyboard.pressed(KeyCode::KeyD) {
            direction.x += 1.0; // Move right
        }

        if direction != Vec3::ZERO {
            direction = direction.normalize();
            camera_transform.translation += direction * speed * time.delta_secs();
        }
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

/// 处理保存和加载输入（演示不同序列化格式）
/// Handle save and load input (demonstrate different serialization formats)
fn handle_save_load_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut save_events: EventWriter<SaveFogOfWarRequest>,
    mut load_events: EventWriter<LoadFogOfWarRequest>,
    mut force_snapshot_events: EventWriter<ForceSnapshotCapturables>,
) {
    // F5键 - 保存雾效数据
    // F5 key - Save fog data
    if keyboard.just_pressed(KeyCode::F5) {
        info!("Saving fog data in multiple formats...");
        save_events.write(SaveFogOfWarRequest {
            include_texture_data: true,
            format: None, // Use default format (prioritizes bincode -> messagepack -> json)
        });
    }

    // F9键 - 加载雾效数据
    // F9 key - Load fog data
    if keyboard.just_pressed(KeyCode::F9) {
        info!("Loading fog data (auto-detect format)...");

        // 尝试不同格式的文件（优先二进制格式）
        // Try different format files (prefer binary formats)
        let format_priorities = vec![
            #[cfg(feature = "format-bincode")]
            "simple_demo.bincode",
            #[cfg(feature = "format-messagepack")]
            "simple_demo.msgpack",
            "simple_demo.json",
        ];

        let mut loaded = false;
        for filename in format_priorities {
            if std::path::Path::new(filename).exists() {
                // 直接读取文件为字节数据
                // Read file as bytes directly
                match std::fs::read(filename) {
                    Ok(data) => {
                        info!("Loading from '{}' (format auto-detected)", filename);
                        load_events.write(LoadFogOfWarRequest {
                            data,
                            format: None, // Auto-detect format from data content
                        });
                        loaded = true;
                        break;
                    }
                    Err(e) => {
                        warn!("Failed to load '{}': {}", filename, e);
                    }
                }
            }
        }

        if !loaded {
            warn!("No save file found. Press F5 to save first.");
        }
    }

    // F12键 - 强制快照屏幕中的所有Capturable实体
    // F12 key - Force snapshot all Capturable entities on screen
    if keyboard.just_pressed(KeyCode::F12) {
        info!("Forcing snapshots for all on-screen Capturable entities...");
        force_snapshot_events.write(ForceSnapshotCapturables);
    }
}

/// 处理保存完成事件（演示多格式保存）
/// Handle save completion event (demonstrate multi-format saving)
fn handle_saved_event(mut events: EventReader<FogOfWarSaved>) {
    for event in events.read() {
        // 直接使用序列化后的二进制数据
        // Use the serialized binary data directly
        let primary_filename = match event.format {
            #[cfg(feature = "format-json")]
            SerializationFormat::Json => "simple_demo.json",
            #[cfg(feature = "format-messagepack")]
            SerializationFormat::MessagePack => "simple_demo.msgpack",
            #[cfg(feature = "format-bincode")]
            SerializationFormat::Bincode => "simple_demo.bincode",
        };

        // 保存主要文件（使用当前格式）
        // Save primary file (using current format)
        match std::fs::write(primary_filename, &event.data) {
            Ok(_) => {
                if let Ok(metadata) = std::fs::metadata(primary_filename) {
                    let size = metadata.len();
                    let size_kb = size as f64 / 1024.0;
                    info!(
                        "✅ Saved {} chunks to '{}': {:.2} KB ({} bytes) - Format: {:?}",
                        event.chunk_count, primary_filename, size_kb, size, event.format
                    );
                }
            }
            Err(e) => {
                error!(
                    "❌ Failed to save fog data to '{}': {}",
                    primary_filename, e
                );
            }
        }
    }
}

/// 处理加载完成事件
/// Handle load completion event
fn handle_loaded_event(mut events: EventReader<FogOfWarLoaded>) {
    for event in events.read() {
        info!("✅ Successfully loaded {} chunks", event.chunk_count);

        if !event.warnings.is_empty() {
            warn!("Load warnings:");
            for warning in &event.warnings {
                warn!("  - {}", warning);
            }
        }
    }
}
