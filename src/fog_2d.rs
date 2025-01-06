use bevy::prelude::*;
use bevy::render::extract_component::{
    ExtractComponent, ExtractComponentPlugin,
};

pub struct FogOfWar2dPlugin;

impl Plugin for FogOfWar2dPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<FogOfWar2dSetting>().add_plugins((
            ExtractComponentPlugin::<FogOfWar2dSetting>::default(),
        ));
    }
}

#[derive(Component, Debug, Clone, Reflect, ExtractComponent)]
pub struct FogOfWar2dSetting {
    pub fog_color: Color,
}

impl Default for FogOfWar2dSetting {
    fn default() -> Self {
        Self {
            fog_color: Color::BLACK,
        }
    }
}
