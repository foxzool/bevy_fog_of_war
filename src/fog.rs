use bevy_color::Color;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use bevy_render::render_resource::Buffer;
use bevy_render_macros::ExtractComponent;

///
#[derive(Component, ExtractComponent, Clone)]
pub struct FogOfWarCamera;

