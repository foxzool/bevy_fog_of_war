use bevy_app::{App, Plugin};

mod chunk;
mod fog_2d;
pub mod prelude;
mod snapshot;
mod sync_texture;
mod vision;

pub struct FogOfWarPlugin;

impl Plugin for FogOfWarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(chunk::ChunkManagerPlugin)
            .add_plugins(vision::VisionComputePlugin)
            .add_plugins(fog_2d::Fog2DRenderPlugin)
            // .add_plugins(snapshot::SnapshotPlugin)
            .add_plugins(sync_texture::GpuSyncTexturePlugin);
    }
}
