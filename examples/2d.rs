use bevy::prelude::*;
use bevy_fog_of_war::{FogOfWar2dPlugin, FogOfWarSettings, FogSight2D};
use std::f32::consts::PI;

// Component to control sight scaling
#[derive(Component)]
struct SightPulse {
    base_radius: f32, // Base radius
    pulse_range: f32, // Scaling range
    speed: f32,       // Scaling speed
    time: f32,        // Accumulated time
}

// Component to mark moving sight
#[derive(Component)]
struct MovingSight {
    speed: f32,
    range: f32,
    center: f32,
}

// Add this new component after other component definitions
#[derive(Component)]
struct CameraController {
    speed: f32,
    chunk_size: f32, // 新增块大小参数
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.9, 0.9, 0.9)))
        .init_gizmo_group::<MyRoundGizmos>()
        .add_plugins(DefaultPlugins)
        // .add_plugins(WorldInspectorPlugin::new())
        .add_plugins(FogOfWar2dPlugin)
        .add_systems(Startup, setup)
        // Add sight scaling system
        .add_systems(
            Update,
            (update_sight_radius, update_sight_position, move_camera),
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
) {
    commands.spawn((
        Camera2d::default(),
        FogOfWarSettings {
            fog_color: Color::BLACK.into(),
            fade_width: 0.2,
            explored_alpha: 0.1, // You can adjust this value to control explored area visibility ,
        },
        CameraController { 
            speed: 500.0,
            chunk_size: 256.0, // 设置默认块大小
        },
        // Transform::from_xyz(0.0, 256.0,0.0),
    ));

    // First sight
    commands.spawn((
        FogSight2D { radius: 100.0 },
        SightPulse {
            base_radius: 100.0,
            pulse_range: 30.0,
            speed: 2.0,
            time: 0.0,
        },
        Transform::from_xyz(-300.0, 0.0, 0.0),
        MovingSight {
            speed: 1.0,
            range: 400.0,
            center: -300.0,
        },
    ));

    // Second sight
    commands.spawn((
        FogSight2D { radius: 200.0 },
        SightPulse {
            base_radius: 250.0,
            pulse_range: 50.0,
            speed: 3.0,
            time: PI,
        },
        // Add transform component
        Transform::from_xyz(100.0, 0.0, 0.0),
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

        commands.spawn((
            Mesh2d(shape),
            MeshMaterial2d(materials.add(color)),
            Transform::from_xyz(
                // Distribute shapes from -X_EXTENT/2 to +X_EXTENT/2.
                -X_EXTENT / 2. + i as f32 / (num_shapes - 1) as f32 * X_EXTENT,
                0.0,
                0.0,
            ),
        ));
    }
}

// System to update sight radius
fn update_sight_radius(time: Res<Time>, mut query: Query<(&mut FogSight2D, &mut SightPulse)>) {
    for (mut sight, mut pulse) in query.iter_mut() {
        // Update accumulated time
        pulse.time += time.delta_secs() * pulse.speed;

        // Calculate current radius using sine function
        let radius_offset = pulse.time.sin() * pulse.pulse_range;
        sight.radius = pulse.base_radius + radius_offset;
    }
}

// Add movement system
fn update_sight_position(time: Res<Time>, mut query: Query<(&mut Transform, &MovingSight)>) {
    for (mut transform, movement) in query.iter_mut() {
        let offset = (time.elapsed_secs() * movement.speed).sin() * movement.range * 0.5;
        transform.translation.x = movement.center + offset;
    }
}

fn move_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut query: Query<(&mut Transform, &CameraController), With<Camera>>,
) {
    if let Ok((mut transform, controller)) = query.get_single_mut() {
        let mut direction = Vec3::ZERO;

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

        // 新增块移动逻辑
        if keyboard.just_pressed(KeyCode::PageUp) {
            transform.translation.y += controller.chunk_size;
        }
        if keyboard.just_pressed(KeyCode::PageDown) {
            transform.translation.y -= controller.chunk_size;
        }

        if direction != Vec3::ZERO {
            direction = direction.normalize();
            transform.translation += direction * controller.speed * time.delta_secs();
        }
    }
}
