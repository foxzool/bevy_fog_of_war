use bevy::{log::LogPlugin, prelude::*};

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.build().disable::<LogPlugin>());
    bevy_mod_debugdump::print_render_graph(&mut app);
}
