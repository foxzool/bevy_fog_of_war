use crate::{
    chunk::ChunkPlugin, chunk_sync::GpuSyncTexturePlugin, fog_2d::ChunkRenderPlugin,
    prelude::FogOfWarCamera, vision::VisionProvider, vision_compute::VisionComputePlugin,
};
use bevy_app::{App, Plugin};
use bevy_render::extract_component::ExtractComponentPlugin;

mod chunk;
mod chunk_sync;
mod fog;
mod fog_2d;
pub mod prelude;
mod snapshot;
mod vision;
mod vision_compute;

pub struct BevyFogOfWarPlugins;

impl Plugin for BevyFogOfWarPlugins {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(ExtractComponentPlugin::<VisionProvider>::default())
            .add_plugins(ExtractComponentPlugin::<FogOfWarCamera>::default())
            .add_plugins(GpuSyncTexturePlugin)
            .add_plugins(ChunkPlugin)
            .add_plugins(VisionComputePlugin)
            .add_plugins(ChunkRenderPlugin)
            .add_plugins(snapshot::SnapshotPlugin);
    }
}
