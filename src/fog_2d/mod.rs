use bevy::core_pipeline::core_2d::graph::{Core2d, Node2d};
use crate::fog_2d::buffers::{FogSight2dBuffers, extract_buffers, prepare_buffers};
use crate::fog_2d::node::{FogOfWar2dNode, FogOfWarLabel};
use crate::fog_2d::pipeline::FogOfWar2dPipeline;

use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin};
use bevy::render::render_graph::{RenderGraphApp, ViewNodeRunner};
use bevy::render::{Render, RenderApp, RenderSet};
use bevy::render::render_resource::ShaderType;
use bytemuck::Pod;
use bytemuck::Zeroable;

mod buffers;
mod node;
mod pipeline;

pub struct FogOfWar2dPlugin;

impl Plugin for FogOfWar2dPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<FogOfWarSettings>().add_plugins((
            ExtractComponentPlugin::<FogOfWarSettings>::default(),
            UniformComponentPlugin::<FogOfWarSettings>::default(),
        ));

        app.register_type::<FogSight2D>()
            .add_plugins((ExtractComponentPlugin::<FogSight2D>::default(),));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<FogSight2dBuffers>()
            .add_systems(ExtractSchedule, extract_buffers)
            .add_systems(Render, (prepare_buffers.in_set(RenderSet::Prepare),))
            .add_render_graph_node::<ViewNodeRunner<FogOfWar2dNode>>(Core2d, FogOfWarLabel)
            .add_render_graph_edges(
                Core2d,
                (
                    Node2d::MainTransparentPass,
                    FogOfWarLabel,
                    Node2d::EndMainPass,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            // Initialize the pipeline
            .init_resource::<FogOfWar2dPipeline>();
    }
}

#[derive(Component, Debug, Clone, Reflect, ExtractComponent, ShaderType)]
pub struct FogOfWarSettings {
    pub fog_color: LinearRgba,
    pub screen_size: Vec2,
    pub fade_width: f32,
}

impl Default for FogOfWarSettings {
    fn default() -> Self {
        Self {
            fog_color: Color::BLACK.into(),
            screen_size: Vec2::new(1280.0, 720.0),
            fade_width: 50.0,
        }
    }
}

#[derive(Component, Debug, Copy, Clone, Reflect, ExtractComponent, ShaderType, Pod, Zeroable)]
#[repr(C)]
pub struct FogSight2D {
    pub position: Vec2,
    pub radius: f32, // 视野的基础半径
}

impl Default for FogSight2D {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            radius: 100.0, // 基础视野半径为100像素
        }
    }
}
