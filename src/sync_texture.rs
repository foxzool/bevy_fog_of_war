use crate::{prelude::ChunkCoord, vision::ExploredTexture};
use async_channel::{Receiver, Sender};
use bevy::{
    prelude::*,
    render::{
        MainWorld, Render, RenderApp, RenderSet,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_asset::RenderAssets,
        render_resource::{
            Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, MapMode,
            Origin3d, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo,
            TextureAspect,
        },
        renderer::{RenderDevice, RenderQueue, render_system},
        sync_world::MainEntity,
        texture::GpuImage,
    },
};

/// A plugin that enables reading back gpu buffers and textures to the cpu.
pub struct GpuSyncTexturePlugin;
impl Plugin for GpuSyncTexturePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<SyncChunk>::default());

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<ImageCopiers>()
                .add_systems(ExtractSchedule, (prepare_download_copier, sync_readbacks))
                .add_systems(
                    Render,
                    (
                        (upload_chunk_texture,).in_set(RenderSet::PrepareResources),
                        (download_chunk_texture, map_buffers)
                            .chain()
                            .after(render_system)
                            .in_set(RenderSet::Render),
                    ),
                );
        }
    }
}
fn upload_chunk_texture(
    render_device: ResMut<RenderDevice>,
    render_queue: Res<RenderQueue>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    explored_texture: Res<ExploredTexture>,
    q_sync_chunks: Query<&SyncChunk>,
) {
    let Some(explored_read) = &explored_texture.read else {
        return;
    };

    for chunk_texture in q_sync_chunks.iter() {
        let mut encoder =
            render_device.create_command_encoder(&CommandEncoderDescriptor::default());
        if !chunk_texture.need_upload {
            continue;
        }
        println!(
            "uploading layer: {} {}",
            chunk_texture.coord, chunk_texture.layer_index
        );

        let src_image = gpu_images.get(&chunk_texture.src).unwrap();

        let block_dimensions = src_image.texture_format.block_dimensions();
        let block_size = src_image.texture_format.block_copy_size(None).unwrap();

        let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(
            (src_image.size.width as usize / block_dimensions.0 as usize) * block_size as usize,
        );

        encoder.copy_buffer_to_texture(
            TexelCopyBufferInfo {
                buffer: &chunk_texture.buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        std::num::NonZero::<u32>::new(padded_bytes_per_row as u32)
                            .unwrap()
                            .into(),
                    ),
                    rows_per_image: None,
                },
            },
            TexelCopyTextureInfo {
                texture: &explored_read.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: chunk_texture.layer_index,
                },
                aspect: TextureAspect::All,
            },
            src_image.size,
        );

        render_queue.submit(std::iter::once(encoder.finish()));
    }
}

fn download_chunk_texture(
    render_device: ResMut<RenderDevice>,
    render_queue: Res<RenderQueue>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    explored_texture: Res<ExploredTexture>,
    image_copiers: Res<ImageCopiers>,
) {
    let Some(explored_write) = &explored_texture.read else {
        return;
    };
    let mut command_encoder =
        render_device.create_command_encoder(&CommandEncoderDescriptor::default());
    for downloader in image_copiers.requested.iter() {
        debug!(
            "downloading layer: {} {}",
            downloader.coord, downloader.layer_index
        );

        let src_image = gpu_images.get(&downloader.src_image).unwrap();

        let block_dimensions = src_image.texture_format.block_dimensions();
        let block_size = src_image.texture_format.block_copy_size(None).unwrap();

        let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(
            (src_image.size.width as usize / block_dimensions.0 as usize) * block_size as usize,
        );

        command_encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &explored_write.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: downloader.layer_index,
                },
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &downloader.buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        std::num::NonZero::<u32>::new(padded_bytes_per_row as u32)
                            .unwrap()
                            .into(),
                    ),
                    rows_per_image: None,
                },
            },
            src_image.size,
        );
    }

    render_queue.submit(std::iter::once(command_encoder.finish()));
}

/// A component that registers the wrapped handle for gpu SyncChunk, either a texture or a buffer.
///
/// Data is read asynchronously and will be triggered on the entity via the [`SyncChunkComplete`]
/// event when complete. If this component is not removed, the SyncChunk will be attempted every
/// frame
#[derive(Component, ExtractComponent, Clone, Debug)]
pub struct SyncChunk {
    pub need_download: bool,
    pub need_upload: bool,
    pub coord: ChunkCoord,
    pub layer_index: u32,
    pub src: Handle<Image>,
    pub buffer: Buffer,
}

impl SyncChunk {
    pub fn new(
        chunk_coord: ChunkCoord,
        src_image: Handle<Image>,
        size: Extent3d,
        render_device: &RenderDevice,
    ) -> SyncChunk {
        let padded_bytes_per_row =
            RenderDevice::align_copy_bytes_per_row((size.width) as usize) * 4;

        let cpu_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some(&format!("{} SyncChunk Buffer", chunk_coord)),
            size: padded_bytes_per_row as u64 * size.height as u64,
            usage: BufferUsages::MAP_READ
                | BufferUsages::MAP_WRITE
                | BufferUsages::COPY_DST
                | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        SyncChunk {
            need_download: false,
            need_upload: false,
            coord: chunk_coord,
            layer_index: 0,
            src: src_image,
            buffer: cpu_buffer,
        }
    }
}

/// An event that is triggered when a gpu SyncChunk is complete.
///
/// The event contains the data as a `Vec<u8>`, which can be interpreted as the raw bytes of the
/// requested buffer or texture.
#[derive(Event, Debug)]
pub struct SyncChunkComplete {
    pub buffer: Buffer,
    pub data: Vec<u8>,
}

/// `ImageCopier` aggregator in `RenderWorld`
#[derive(Clone, Default, Resource)]
struct ImageCopiers {
    requested: Vec<ImageCopier>,
    mapped: Vec<ImageCopier>,
}

/// Used by `ImageCopyDriver` for copying from render target to buffer
#[derive(Clone, Component)]
struct ImageCopier {
    entity: Entity,
    coord: ChunkCoord,
    layer_index: u32,
    buffer: Buffer,
    src_image: Handle<Image>,
    pub rx: Receiver<(Entity, Buffer, Vec<u8>)>,
    pub tx: Sender<(Entity, Buffer, Vec<u8>)>,
}

fn sync_readbacks(mut main_world: ResMut<MainWorld>, mut image_copiers: ResMut<ImageCopiers>) {
    image_copiers.mapped.retain(|readback| {
        if let Ok((entity, buffer, result)) = readback.rx.try_recv() {
            main_world.trigger_targets(
                SyncChunkComplete {
                    buffer,
                    data: result,
                },
                entity,
            );
            false
        } else {
            true
        }
    });
}

fn prepare_download_copier(
    handles: Query<(&MainEntity, &SyncChunk)>,
    mut image_copiers: ResMut<ImageCopiers>,
) {
    for (main_entity, sync_chunk) in handles.iter() {
        if sync_chunk.need_download {
            let (tx, rx) = async_channel::bounded(1);
            image_copiers.requested.push(ImageCopier {
                entity: main_entity.id(),
                coord: sync_chunk.coord,
                layer_index: sync_chunk.layer_index,
                buffer: sync_chunk.buffer.clone(),
                src_image: sync_chunk.src.clone(),
                tx,
                rx,
            });
        }
    }
}

fn map_buffers(mut image_copiers: ResMut<ImageCopiers>) {
    let requested = image_copiers
        .requested
        .drain(..)
        .collect::<Vec<ImageCopier>>();
    for image_copier in requested {
        let slice = image_copier.buffer.slice(..);
        let entity = image_copier.entity;
        let buffer = image_copier.buffer.clone();
        let tx = image_copier.tx.clone();
        slice.map_async(MapMode::Read, move |res| {
            res.expect("Failed to map buffer");
            let buffer_slice = buffer.slice(..);
            let data = buffer_slice.get_mapped_range();
            let result = Vec::from(&*data);
            drop(data);
            buffer.unmap();
            if let Err(e) = tx.try_send((entity, buffer, result)) {
                warn!("Failed to send readback result: {}", e);
            }
        });
        image_copiers.mapped.push(image_copier.clone());
    }
}
