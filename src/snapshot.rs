use crate::chunk::ChunkManager;
use crate::render::{FogNode2d, FogNode2dLabel};
use bevy_app::Plugin;
use bevy_core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy_diagnostic::FrameCount;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryItem;
use bevy_render::render_graph::{NodeRunError, RenderGraphApp, RenderGraphContext, ViewNode, ViewNodeRunner};
use bevy_render::render_resource::{
    Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};
use bevy_render::renderer::{RenderContext, RenderDevice};
use bevy_render::texture::{CachedTexture, TextureCache};
use bevy_render::{Extract, ExtractSchedule, RenderApp};
use bevy_render_macros::RenderLabel;

pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<SnapshotTexture>()
            .add_systems(ExtractSchedule, prepare_snapshot_texture)
            .add_render_graph_node::<ViewNodeRunner<SnapshotNode>>(Core2d, SnapshotNodeLabel)
            .add_render_graph_edges(
                Core2d,
                (FogNode2dLabel, SnapshotNodeLabel, Node2d::EndMainPass),
            );
    }
}


#[derive(Resource, Default)]
struct SnapshotTexture {
    write: Option<CachedTexture>,
    read: Option<CachedTexture>,
}

fn prepare_snapshot_texture(
    mut texture_cache: ResMut<TextureCache>,
    render_device: Res<RenderDevice>,
    frame_count: Extract<Res<FrameCount>>,
    chunk_manager: Extract<Res<ChunkManager>>,
    mut commands: Commands,
) {
    let width = chunk_manager.chunk_size.x * chunk_manager.tile_size as u32;
    let height = chunk_manager.chunk_size.y * chunk_manager.tile_size as u32;
    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: chunk_manager.max_layer_count,
    };

    let mut texture_descriptor = TextureDescriptor {
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::COPY_DST | TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
        label: None,
        view_formats: &[],
    };
    texture_descriptor.label = Some("snap_1_texture");
    let history_1_texture = texture_cache.get(&render_device, texture_descriptor.clone());

    texture_descriptor.label = Some("snap_2_texture");
    let history_2_texture = texture_cache.get(&render_device, texture_descriptor.clone());

    let texture = if frame_count.0 % 2 == 0 {
        SnapshotTexture {
            write: Some(history_1_texture),
            read: Some(history_2_texture),
        }
    } else {
        SnapshotTexture {
            write: Some(history_2_texture),
            read: Some(history_1_texture),
        }
    };

    commands.insert_resource(texture);
}


#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct SnapshotNodeLabel;

#[derive(Default)]
struct SnapshotNode;

impl ViewNode for SnapshotNode {
    type ViewQuery = ();

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let snapshot_texture = world.resource::<SnapshotTexture>();
        let snapshot_texture = world.resource::<SnapshotTexture>();

        let (Some(snap_read), Some(snap_write)) = (&snapshot_texture.read, &snapshot_texture.write) else {
            return Ok(());
        };
        Ok(())
    }
}