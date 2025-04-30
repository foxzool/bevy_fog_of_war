use crate::{
    chunk::ChunkManagerPlugin, chunk_sync::GpuSyncTexturePlugin, fog_2d::Fog2DRenderPlugin,
    prelude::FogOfWarCamera,
};
use bevy_app::{App, Plugin};
use bevy_render::extract_component::ExtractComponentPlugin;

mod chunk;
mod chunk_sync;
mod fog_2d;
pub mod prelude;
mod snapshot;
mod vision;

pub struct FogOfWarPlugin;

impl Plugin for FogOfWarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<FogOfWarCamera>::default())
            .add_plugins(ChunkManagerPlugin)
            .add_plugins(vision::VisionComputePlugin)
            .add_plugins(Fog2DRenderPlugin)
            .add_plugins(snapshot::SnapshotPlugin)
            .add_plugins(GpuSyncTexturePlugin);
    }
}
