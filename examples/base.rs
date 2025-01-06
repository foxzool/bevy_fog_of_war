use bevy::prelude::*;
use fog_of_war::FogOfWarPlugin;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.9, 0.9, 0.9)))
        .add_plugins(DefaultPlugins)
        .add_plugins(FogOfWarPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, draw_debug_line)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d::default());
}

fn draw_debug_line(mut gizmos: Gizmos) {
    gizmos
        .grid_2d(
            Isometry2d::IDENTITY,
            UVec2::new(16, 9),
            Vec2::new(100., 100.),
            // Dark gray
            LinearRgba::gray(0.05),
        )
        .outer_edges();
}
