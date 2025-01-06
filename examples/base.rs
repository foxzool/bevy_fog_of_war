use bevy::prelude::*;
use fog_of_war::FogOfWarPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(FogOfWarPlugin)
        .run();
}
