use bevy::prelude::*;
use bevy_fog_of_war::prelude::{
    Capturable, FogMapSettings, FogOfWarCamera, FogOfWarPlugin, FogResetFailed, FogResetSuccess,
    VisionSource,
};

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::WHITE))
        .init_gizmo_group::<MyRoundGizmos>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Simple 2d Fog of War ".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(FogOfWarPlugin)
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (draw_gizmos, camera_movement, handle_fog_reset_events),
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
