use bevy::prelude::*;

mod chunk;
mod systems;

pub use chunk::{Chunk, ChunkManager};
use systems::{check_and_generate_chunks_in_view, process_chunk_generation, debug_draw_chunk_boundaries};

/// 迷雾战争插件配置
/// Fog of War plugin configuration
#[derive(Resource)]
pub struct FogOfWarConfig {
    /// 区块大小（世界单位）
    /// Chunk size (world units)
    pub chunk_size: f32,
    /// 视野范围（以区块为单位）
    /// View range (in chunks)
    pub view_range: u32,
    /// 是否启用调试绘制
    /// Whether to enable debug drawing
    pub debug_draw: bool,
}

impl Default for FogOfWarConfig {
    fn default() -> Self {
        Self {
            chunk_size: 256.0,
            view_range: 3,
            debug_draw: true,
        }
    }
}

pub struct FogOfWarPlugin;

impl Plugin for FogOfWarPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<FogOfWarConfig>()
            .init_resource::<ChunkManager>()
            .add_systems(Update, (
                check_and_generate_chunks_in_view,
                process_chunk_generation,
            ))
            .add_systems(Update, debug_draw_chunk_boundaries.run_if(|config: Res<FogOfWarConfig>| config.debug_draw));
    }
}

/// 在启动时设置迷雾战争插件配置
/// Set up Fog of War plugin configuration at startup
pub fn setup_fog_of_war(mut commands: Commands, config: ResMut<FogOfWarConfig>) {
    // 创建区块管理器
    // Create chunk manager
    commands.insert_resource(ChunkManager::new(
        config.chunk_size,
        config.view_range,
    ));
}