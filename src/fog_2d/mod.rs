use crate::fog_2d::buffers::{
    extract_buffers, prepare_buffers, prepare_chunk_texture, prepare_settings_buffer,
    FogOfWarSettingBuffer, FogSight2dBuffers,
};
use crate::fog_2d::chunk::{
    debug_chunk_indices, update_chunk_array_indices, update_chunks_system, ChunkArrayIndex,
    ChunkCoord,
};
use crate::fog_2d::node::{FogOfWar2dNode, FogOfWarLabel};
use crate::fog_2d::pipeline::FogOfWar2dPipeline;
use bevy::asset::load_internal_asset;
use bevy::color::palettes::basic::{BLUE, RED, YELLOW};
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
use bevy_render::view::need_surface_configuration;

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
            .init_resource::<FogOfWarSettings>();

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_resource::<FogOfWarSettingBuffer>();
        }

        app.add_systems(
            Update,
            (
                update_chunk_array_indices,
                update_chunks_system,
                debug_chunk_indices,
                draw_chunk_boundaries,
            ),
        )
        .add_plugins((
            ExtractComponentPlugin::<FogOfWarCamera>::default(),
            ExtractComponentPlugin::<ChunkCoord>::default(),
            ExtractComponentPlugin::<ChunkArrayIndex>::default(),
            ExtractResourcePlugin::<FogOfWarSettings>::default(),
        ));

        app.register_type::<FogSight2D>();

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<FogSight2dBuffers>()
            .add_systems(ExtractSchedule, extract_buffers)
            .add_systems(Render, create_pipeline.run_if(need_surface_configuration))
            .add_systems(
                Render,
                ((
                    prepare_buffers,
                    prepare_settings_buffer,
                    prepare_chunk_texture,
                )
                    .in_set(RenderSet::PrepareResources),),
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
}

#[derive(Component, Clone, ExtractComponent)]
pub struct FogOfWarCamera;

#[derive(Resource, Debug, Clone, Reflect, ExtractResource, ShaderType)]
pub struct FogOfWarSettings {
    pub chunk_size: f32,
    pub fog_color: LinearRgba,
    pub fade_width: f32,
    pub explored_alpha: f32,
}

impl Default for FogOfWarSettings {
    fn default() -> Self {
        Self {
            chunk_size: 256.0,
            fog_color: Color::BLACK.into(),
            fade_width: 10.0,
            explored_alpha: 0.1,
        }
    }
}

pub fn calculate_max_chunks(size: Vec2, chunk_size: f32) -> (u32, u32) {
    let max_chunks_x = (size.x / chunk_size).ceil() as u32;
    let max_chunks_y = (size.y / chunk_size).ceil() as u32;
    (max_chunks_x, max_chunks_y)
}

/// 独立计算视野内区块坐标的方法
/// 参数说明:
/// - screen_size: 屏幕尺寸 (UVec2)
/// - camera_pos: 相机位置 (Vec2)
/// - chunk_size: 区块大小 (f32)
pub fn calculate_visible_chunks(
    screen_size: UVec2,
    camera_pos: Vec2,
    chunk_size: f32,
) -> Vec<ChunkCoord> {
    let half_width = screen_size.x as f32 * 0.5;
    let half_height = screen_size.y as f32 * 0.5;
    let min_x = camera_pos.x - half_width;
    let max_x = camera_pos.x + half_width;
    let min_y = camera_pos.y - half_height;
    let max_y = camera_pos.y + half_height;

    // 转换为区块坐标并扩展1个区块确保覆盖
    let start_chunk_x = (min_x as i32).div_euclid(chunk_size as i32) - 1;
    let end_chunk_x = (max_x as i32).div_euclid(chunk_size as i32) + 1;
    let start_chunk_y = (min_y as i32).div_euclid(chunk_size as i32) - 1;
    let end_chunk_y = (max_y as i32).div_euclid(chunk_size as i32) + 1;

    // 收集所有与可视区域相交的区块
    let mut chunks_in_view = Vec::new();
    for x in start_chunk_x..=end_chunk_x {
        for y in start_chunk_y..=end_chunk_y {
            chunks_in_view.push(ChunkCoord::new(x, y));
        }
    }

    chunks_in_view
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

fn draw_chunk_boundaries(
    chunks_query: Query<(&ChunkCoord, &ChunkArrayIndex)>,
    settings: Res<FogOfWarSettings>,
    mut gizmos: Gizmos,
) {
    if cfg!(feature = "debug_chunk") {
        for (chunk_coord, chunk_index) in chunks_query.iter() {
            if chunk_index.current != Some(17) {
                continue;
            }
            let world_pos = chunk_coord.to_world_pos(settings.chunk_size);
            let chunk_size = settings.chunk_size;
            gizmos.circle_2d(world_pos, 10.0, BLUE);
            // 使用左上角作为矩形的起点
            gizmos.rect_2d(
                Vec2::new(
                    world_pos.x + chunk_size * 0.5,
                    world_pos.y - chunk_size * 0.5,
                ), // 中心点需要偏移半个chunk大小
                Vec2::splat(chunk_size),
                YELLOW,
            );
        }
    }
}

fn create_pipeline(mut commands: Commands) {
    commands.init_resource::<FogOfWar2dPipeline>();
}
