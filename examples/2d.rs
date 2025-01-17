use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use fog_of_war::{FogOfWar2dPlugin, FogOfWarSettings, FogSight2D};
use std::f32::consts::PI;

// 添加这个组件来控制视野的缩放
#[derive(Component)]
struct SightPulse {
    base_radius: f32,     // 基础半径
    pulse_range: f32,     // 缩放范围
    speed: f32,          // 缩放速度
    time: f32,           // 累计时间
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.9, 0.9, 0.9)))
        .add_plugins(DefaultPlugins)
        .add_plugins(WorldInspectorPlugin::new())
        .add_plugins(FogOfWar2dPlugin)
        .add_systems(Startup, setup)
        // 添加视野缩放系统
        .add_systems(Update, update_sight_radius)
        .run();
}

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
            fog_color: Color::linear_rgba(0.0, 0.0, 0.0, 0.95).into(),
            screen_size: primary_window.size(),
            fade_width: 50.0,  // 全局过渡范围设置
        },
    ));

    // 修改视野点的生成，添加缩放组件
    commands.spawn((
        FogSight2D {
            position: Vec2::new(-100.0, 0.0),
            radius: 100.0,
        },
        SightPulse {
            base_radius: 100.0,
            pulse_range: 30.0,  // 半径将在 70-130 之间变化
            speed: 2.0,        // 控制缩放速度
            time: 0.0,
        },
    ));

    commands.spawn((
        FogSight2D {
            position: Vec2::new(100.0, 0.0),
            radius: 150.0,
        },
        SightPulse {
            base_radius: 150.0,
            pulse_range: 50.0,  // 半径将在 100-200 之间变化
            speed: 3.0,        // 不同的缩放速度
            time: PI,          // 不同的初始相位
        },
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
fn update_sight_radius(
    time: Res<Time>,
    mut query: Query<(&mut FogSight2D, &mut SightPulse)>,
) {
    for (mut sight, mut pulse) in query.iter_mut() {
        // 更新累计时间
        pulse.time += time.delta_secs() * pulse.speed;
        
        // 使用正弦函数计算当前半径
        let radius_offset = (pulse.time.sin() * pulse.pulse_range);
        sight.radius = pulse.base_radius + radius_offset;
    }
}
