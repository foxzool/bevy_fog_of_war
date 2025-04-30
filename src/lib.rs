use crate::{
    chunk::ChunkPlugin, chunk_sync::GpuSyncTexturePlugin, fog::FogMaterial,
    prelude::FogOfWarCamera, render::ChunkRenderPlugin, vision::VisionProvider,
    vision_compute::VisionComputePlugin,
};
use bevy_app::{App, Plugin};
use bevy_render::extract_component::ExtractComponentPlugin;

mod chunk;
mod chunk_sync;
mod fog;
pub mod prelude;
mod render;
mod vision;
mod vision_compute;
mod snapshot;

pub struct BevyFogOfWarPlugins;

impl Plugin for BevyFogOfWarPlugins {
    fn build(&self, app: &mut App) {
        app.register_type::<FogMaterial>()
            .add_plugins(ExtractComponentPlugin::<FogMaterial>::default())
            .add_plugins(ExtractComponentPlugin::<VisionProvider>::default())
            .add_plugins(ExtractComponentPlugin::<FogOfWarCamera>::default())
            .add_plugins(GpuSyncTexturePlugin)
            .add_plugins(ChunkPlugin)
            .add_plugins(VisionComputePlugin)
            .add_plugins(ChunkRenderPlugin);
    }
}
