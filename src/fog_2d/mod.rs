use crate::fog_2d::buffers::{
    extract_buffers, prepare_buffers, prepare_chunk_texture, FogSight2dBuffers,
    FogSight2dScreenBuffers,
};
use crate::fog_2d::chunk::{update_chunk_array_indices, update_chunks_system, ChunkArrayIndex, ChunkCoord, CHUNK_SIZE};
use crate::fog_2d::node::{FogOfWar2dNode, FogOfWarLabel};
use crate::fog_2d::pipeline::FogOfWar2dPipeline;
use bevy::asset::load_internal_asset;
use bevy::core_pipeline::core_2d::graph::{Core2d, Node2d};

use bevy::prelude::*;
use bevy::render::extract_component::{
    ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin,
};
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_graph::{RenderGraphApp, ViewNodeRunner};
use bevy::render::render_resource::ShaderType;
use bevy::render::{Render, RenderApp, RenderSet};
use bevy::window::WindowResized;

mod buffers;
mod chunk;
mod node;
mod pipeline;

pub const FOG_OF_WAR_2D_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(2645352199453808407);

pub struct FogOfWar2dPlugin;

impl Plugin for FogOfWar2dPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            FOG_OF_WAR_2D_SHADER_HANDLE,
            "fog_of_war_2d.wgsl",
            Shader::from_wgsl
        );

        app.register_type::<FogOfWarSettings>()
            .register_type::<FogOfWarScreen>()
            .init_resource::<FogOfWarScreen>()
            .add_systems(
                Update,
                (
                    adjust_fow_screen,
                    update_chunk_array_indices,
                    update_chunks_system.run_if(resource_changed::<FogOfWarScreen>),
                ),
            )
            .add_plugins((
                ExtractComponentPlugin::<ChunkCoord>::default(),
                ExtractComponentPlugin::<ChunkArrayIndex>::default(),
                ExtractComponentPlugin::<FogOfWarSettings>::default(),
                ExtractResourcePlugin::<FogOfWarScreen>::default(),
                UniformComponentPlugin::<FogOfWarSettings>::default(),
            ));

        app.register_type::<FogSight2D>();

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<FogSight2dBuffers>()
            .init_resource::<FogSight2dScreenBuffers>()
            .add_systems(ExtractSchedule, extract_buffers)
            .add_systems(
                Render,
                ((prepare_buffers, prepare_chunk_texture).in_set(RenderSet::Prepare),),
            )
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
    pub fade_width: f32,
    pub explored_alpha: f32,
    pub debug_chunks: u32,
}

impl Default for FogOfWarSettings {
    fn default() -> Self {
        Self {
            fog_color: Color::BLACK.into(),
            fade_width: 50.0,
            explored_alpha: 0.5,
            debug_chunks: 0,
        }
    }
}

#[derive(Resource, Component, Debug, Clone, Reflect, ExtractResource, ShaderType)]
pub struct FogOfWarScreen {
    pub screen_size: Vec2,
    pub camera_position: Vec2,
    pub chunk_size: f32,
}

impl Default for FogOfWarScreen {
    fn default() -> Self {
        Self {
            screen_size: Vec2::ZERO,
            camera_position: Vec2::ZERO,
            chunk_size: CHUNK_SIZE as f32,
        }
    }
}

impl FogOfWarScreen {
    pub fn calculate_max_chunks(&self) -> (u32, u32) {
        let max_chunks_x = ((self.screen_size.x / self.chunk_size).ceil() as u32) + 1;
        let max_chunks_y = ((self.screen_size.y / self.chunk_size).ceil() as u32) + 1;

        (max_chunks_x, max_chunks_y)
    }

    fn get_chunks_in_view(&self) -> Vec<ChunkCoord> {
        let half_width = self.screen_size.x * 0.5;
        let half_height = self.screen_size.y * 0.5;
        let min_x = self.camera_position.x - half_width;
        let max_x = self.camera_position.x + half_width;
        let min_y = self.camera_position.y - half_height;
        let max_y = self.camera_position.y + half_height;

        // Convert to chunk coordinates and add 1 to ensure coverage
        let start_chunk_x = (min_x as i32).div_euclid(CHUNK_SIZE as i32) - 1;
        let end_chunk_x = (max_x as i32).div_euclid(CHUNK_SIZE as i32) + 1;
        let start_chunk_y = (min_y as i32).div_euclid(CHUNK_SIZE as i32) - 1;
        let end_chunk_y = (max_y as i32).div_euclid(CHUNK_SIZE as i32) + 1;

        // Collect all chunks that intersect with the visible area
        let mut chunks_in_view = Vec::new();
        for x in start_chunk_x..=end_chunk_x {
            for y in start_chunk_y..=end_chunk_y {
                chunks_in_view.push(ChunkCoord::new(x, y));
            }
        }

        chunks_in_view
    }
}

#[derive(Component, Reflect, Debug)]
pub struct FogSight2D {
    pub radius: f32, // 视野的基础半径
}

impl Default for FogSight2D {
    fn default() -> Self {
        Self {
            radius: 100.0, // 基础视野半径为100像素
        }
    }
}

// Render component
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
#[repr(C)]
pub struct FogSight2DUniform {
    pub position: Vec2,
    pub radius: f32,
}

pub fn adjust_fow_screen(
    mut fow_screen: ResMut<FogOfWarScreen>,
    mut resize_events: EventReader<WindowResized>,
    camera_query: Query<(&Camera, &GlobalTransform), Changed<GlobalTransform>>,
) {
    // Update screen size on window resize
    for event in resize_events.read() {
        debug!("window resized to {}x{}", event.width, event.height);
        fow_screen.screen_size = Vec2::new(event.width, event.height);
    }

    // Update camera position
    if let Ok((_, transform)) = camera_query.get_single() {
        fow_screen.camera_position = transform.translation().truncate();
    }
}
