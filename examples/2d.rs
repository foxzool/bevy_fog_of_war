use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use fog_of_war::{FogOfWar2dPlugin, FogOfWarSettings, FogSight2D};
use std::f32::consts::PI;

// 添加这个组件来控制视野的缩放
#[derive(Component)]
struct SightPulse {
    base_radius: f32, // 基础半径
    pulse_range: f32, // 缩放范围
    speed: f32,       // 缩放速度
    time: f32,        // 累计时间
}

// 添加一个新的组件来标记移动的遮罩
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
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.9, 0.9, 0.9)))
        .init_gizmo_group::<MyRoundGizmos>()
        .add_plugins(DefaultPlugins)
        .add_plugins(WorldInspectorPlugin::new())
        .add_plugins(FogOfWar2dPlugin)
        .add_systems(Startup, setup)
        // 添加视野缩放系统
        .add_systems(
            Update,
            (
                update_sight_radius,
                update_sight_position,
                draw_grid,
                move_camera, // Add the new camera movement system
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
    primary_window: Single<&Window, With<PrimaryWindow>>,
) {
    commands.spawn((
        Camera2d::default(),
        FogOfWarSettings {
            fog_color: Color::linear_rgba(0.0, 0.0, 0.0, 1.0).into(),
            screen_size: primary_window.size(),
            fade_width: 0.2,
            explored_alpha: 0.1, // You can adjust this value to control explored area visibility
        },
        CameraController { speed: 500.0 }, // Add camera controller
    ));

    // First sight
    commands.spawn((
        FogSight2D {
            radius: 100.0,
            position: Vec2::ZERO, // Position will be overridden by transform
        },
        SightPulse {
            base_radius: 100.0,
            pulse_range: 30.0,
            speed: 2.0,
            time: 0.0,
        },
        // Add transform component
        Transform::from_xyz(-300.0, 0.0, 0.0),
        MovingSight {
            speed: 1.0,
            range: 400.0,
            center: -300.0,
        },
    ));

    // Second sight
    commands.spawn((
        FogSight2D {
            radius: 150.0,
            position: Vec2::ZERO,
        },
        SightPulse {
            base_radius: 150.0,
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

// 更新视野半径的系统
fn update_sight_radius(time: Res<Time>, mut query: Query<(&mut FogSight2D, &mut SightPulse)>) {
    for (mut sight, mut pulse) in query.iter_mut() {
        // 更新累计时间
        pulse.time += time.delta_secs() * pulse.speed;

        // 使用正弦函数计算当前半径
        let radius_offset = pulse.time.sin() * pulse.pulse_range;
        sight.radius = pulse.base_radius + radius_offset;
    }
}

// 添加移动系统
fn update_sight_position(time: Res<Time>, mut query: Query<(&mut Transform, &MovingSight)>) {
    for (mut transform, movement) in query.iter_mut() {
        let offset = (time.elapsed_secs() * movement.speed).sin() * movement.range * 0.5;
        transform.translation.x = movement.center + offset;
    }
}

fn draw_grid(mut gizmos: Gizmos) {
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

        if direction != Vec3::ZERO {
            direction = direction.normalize();
            transform.translation += direction * controller.speed * time.delta_secs();
        }
    }
}
