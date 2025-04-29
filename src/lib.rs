use crate::chunk_sync::GpuSyncTexturePlugin;
use crate::gpu_sync_chunk::GpuSyncChunkPlugin;
use crate::{
    chunk::ChunkPlugin, fog::FogMaterial, prelude::FogOfWarCamera, render::ChunkRenderPlugin,
    vision::VisionProvider, vision_compute::VisionComputePlugin,
};
use bevy_app::{App, Plugin};
use bevy_render::extract_component::ExtractComponentPlugin;

mod chunk;
mod chunk_sync;
mod fog;
mod gpu_sync_chunk;
pub mod prelude;
mod render;
mod vision;
mod vision_compute;

pub struct ZingFogPlugins;

impl Plugin for ZingFogPlugins {
    fn build(&self, app: &mut App) {
        app.register_type::<FogMaterial>()
            .add_plugins(ExtractComponentPlugin::<FogMaterial>::default())
            .add_plugins(ExtractComponentPlugin::<VisionProvider>::default())
            .add_plugins(ExtractComponentPlugin::<FogOfWarCamera>::default())
            // .add_plugins(GpuSyncChunkPlugin::default())
            .add_plugins(GpuSyncTexturePlugin::default())
            .add_plugins(ChunkPlugin)
            .add_plugins(VisionComputePlugin)
            .add_plugins(ChunkRenderPlugin);
    }
}
