use crate::fog_2d::buffers::{
    extract_buffers, prepare_buffers, prepare_chunk_texture, FogSight2dBuffers,
    FogSight2dScreenBuffers,
};
use crate::fog_2d::chunk::{update_chunk_array_indices, update_chunks_system, ChunkCoord, CHUNK_SIZE};
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
}

impl Default for FogOfWarSettings {
    fn default() -> Self {
        Self {
            fog_color: Color::BLACK.into(),
            fade_width: 50.0,
            explored_alpha: 0.5,
        }
    }
}

#[derive(Resource, Component, Debug, Clone, Reflect, ExtractResource, ShaderType)]
pub struct FogOfWarScreen {
    pub screen_size: Vec2,
    pub camera_position: Vec2,
    pub chunk_size: u32,
    pub view_start_chunk: Vec2, // 当前视图的起始chunk坐标
}

impl Default for FogOfWarScreen {
    fn default() -> Self {
        Self {
            screen_size: Vec2::ZERO,
            camera_position: Vec2::ZERO,
            chunk_size: CHUNK_SIZE,
            view_start_chunk: Vec2::ZERO,
        }
    }
}

impl FogOfWarScreen {
    /// Calculates the maximum number of chunks that can fit within the current window size.
    ///
    /// This function computes the number of chunks along both the x and y axes
    /// based on the window size and the predefined chunk size. An additional
    /// chunk is added to ensure complete coverage.
    ///
    /// Returns:
    /// A tuple containing:
    /// - `max_chunks_x`: The maximum number of chunks along the x-axis.
    /// - `max_chunks_y`: The maximum number of chunks along the y-axis.
    pub fn calculate_max_chunks(&self) -> (u32, u32) {
        let max_chunks_x = ((self.screen_size.x / self.chunk_size as f32).ceil() as u32) + 1;
        let max_chunks_y = ((self.screen_size.y / self.chunk_size as f32).ceil() as u32) + 1;

        (max_chunks_x, max_chunks_y)
    }

    pub fn update_view_start(&mut self) {
        let half_width = self.screen_size.x * 0.5;
        let half_height = self.screen_size.y * 0.5;
        let min_x = self.camera_position.x - half_width;
        let min_y = self.camera_position.y - half_height;
        
        // 计算新的视图起始chunk坐标
        self.view_start_chunk = Vec2::new(
            (min_x / self.chunk_size as f32).floor() - 1.0,
            (min_y / self.chunk_size as f32).floor() - 1.0,
        );
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
        fow_screen.update_view_start(); // Update view start when screen size changes
    }

    // Update camera position and view start
    if let Ok((_, transform)) = camera_query.get_single() {
        let old_camera_pos = fow_screen.camera_position;
        fow_screen.camera_position = transform.translation().truncate();
        
        // 如果相机移动超过一定距离，更新view_start_chunk
        let movement = fow_screen.camera_position - old_camera_pos;
        if movement.length() > fow_screen.chunk_size as f32 * 0.5 {
            fow_screen.update_view_start();
        }
    }
}
