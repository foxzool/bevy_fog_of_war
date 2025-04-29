use crate::prelude::{ChunkCoord, MapChunk};
use crate::vision_compute::ExploredTexture;
use async_channel::{Receiver, Sender};
use bevy_app::{App, Plugin};
use bevy_asset::{Assets, Handle};
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::{Commands, resource_exists};
use bevy_ecs::{
    change_detection::ResMut,
    entity::Entity,
    event::Event,
    prelude::{Component, Resource},
    schedule::IntoScheduleConfigs,
    system::{Query, Res},
};
use bevy_image::{Image, TextureFormatPixelInfo};
use bevy_log::warn;
use bevy_platform::collections::HashMap;
use bevy_reflect::Reflect;
use bevy_render::render_resource::{
    CommandEncoderDescriptor, Origin3d, TexelCopyTextureInfo, TextureAspect,
};
use bevy_render::renderer::RenderQueue;
use bevy_render::{
    ExtractSchedule, MainWorld, Render, RenderApp, RenderSet,
    extract_component::ExtractComponentPlugin,
    render_asset::RenderAssets,
    render_resource::{
        Buffer, BufferDescriptor, BufferUsages, CommandEncoder, Extent3d, MapMode,
        TexelCopyBufferInfo, TexelCopyBufferLayout, Texture, TextureFormat,
    },
    renderer::{RenderDevice, render_system},
    storage::GpuShaderStorageBuffer,
    sync_world::MainEntity,
    texture::GpuImage,
};
use bevy_render_macros::ExtractComponent;
use encase::ShaderType;
use encase::internal::ReadFrom;
use encase::private::Reader;

/// A plugin that enables reading back gpu buffers and textures to the cpu.
pub struct GpuSyncTexturePlugin {
    /// Describes the number of frames a buffer can be unused before it is removed from the pool to avoid unnecessary reallocations.
    max_unused_frames: usize,
}

impl Default for GpuSyncTexturePlugin {
    fn default() -> Self {
        Self {
            max_unused_frames: 10,
        }
    }
}

impl Plugin for GpuSyncTexturePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<SyncChunk>::default());

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<GpuSyncChunkBufferPool>()
                .init_resource::<GpuSyncChunks>()
                .init_resource::<ImageCopiers>()
                .insert_resource(GpuSyncChunkMaxUnusedFrames(self.max_unused_frames))
                .add_systems(ExtractSchedule, (texture_copy_extract, sync_readbacks))
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
    let (Some(explored_read), Some(explored_write)) =
        (&explored_texture.read, &explored_texture.write)
    else {
        return;
    };
    let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor::default());
    for chunk_texture in q_sync_chunks.iter() {
        if !chunk_texture.need_upload {
            continue;
        }
        println!(
            "uploading layer: {} {}",
            chunk_texture.coord, chunk_texture.layer_index
        );
        if chunk_texture.coord == ChunkCoord::new(-1, -1) {

        }
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
    }
    render_queue.submit(std::iter::once(encoder.finish()));
}

fn download_chunk_texture(
    render_device: ResMut<RenderDevice>,
    render_queue: Res<RenderQueue>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    sync_chunks: Res<GpuSyncChunks>,
    explored_texture: Res<ExploredTexture>,
    image_copiers: Res<ImageCopiers>,
) {
    let (Some(explored_read), Some(explored_write)) =
        (&explored_texture.read, &explored_texture.write)
    else {
        return;
    };
    let mut command_encoder =
        render_device.create_command_encoder(&CommandEncoderDescriptor::default());
    for downloader in image_copiers.requested.iter() {
        println!(
            "downloading layer: {} {}",
            downloader.coord, downloader.layer_index
        );
        if downloader.coord == ChunkCoord::new(-1, -1) {

        }

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
/// Data is read asynchronously and will be triggered on the entity via the [`SyncChunkComplete`] event
/// when complete. If this component is not removed, the SyncChunk will be attempted every frame
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

impl SyncChunkComplete {
    /// Convert the raw bytes of the event to a shader type.
    pub fn to_shader_type<T: ShaderType + ReadFrom + Default>(&self) -> T {
        let mut val = T::default();
        let mut reader = Reader::new::<T>(&self.data, 0).expect("Failed to create Reader");
        T::read_from(&mut val, &mut reader);
        val
    }
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

#[derive(Resource)]
struct GpuSyncChunkMaxUnusedFrames(usize);

struct GpuSyncChunkBuffer {
    buffer: Buffer,
    taken: bool,
    frames_unused: usize,
}

#[derive(Resource, Default)]
struct GpuSyncChunkBufferPool {
    buffers: HashMap<u64, Vec<GpuSyncChunkBuffer>>,
}

impl GpuSyncChunkBufferPool {
    fn get(&mut self, render_device: &RenderDevice, size: u64) -> Buffer {
        let buffers = self.buffers.entry(size).or_default();

        // find an untaken buffer for this size
        if let Some(buf) = buffers.iter_mut().find(|x| !x.taken) {
            buf.taken = true;
            buf.frames_unused = 0;
            return buf.buffer.clone();
        }

        let buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("SyncChunk Buffer"),
            size,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        buffers.push(GpuSyncChunkBuffer {
            buffer: buffer.clone(),
            taken: true,
            frames_unused: 0,
        });
        buffer
    }

    // Returns the buffer to the pool so it can be used in a future frame
    fn return_buffer(&mut self, buffer: &Buffer) {
        let size = buffer.size();
        let buffers = self
            .buffers
            .get_mut(&size)
            .expect("Returned buffer of untracked size");
        if let Some(buf) = buffers.iter_mut().find(|x| x.buffer.id() == buffer.id()) {
            buf.taken = false;
        } else {
            warn!("Returned buffer that was not allocated");
        }
    }

    fn update(&mut self, max_unused_frames: usize) {
        for (_, buffers) in &mut self.buffers {
            // Tick all the buffers
            for buf in &mut *buffers {
                if !buf.taken {
                    buf.frames_unused += 1;
                }
            }

            // Remove buffers that haven't been used for MAX_UNUSED_FRAMES
            buffers.retain(|x| x.frames_unused < max_unused_frames);
        }

        // Remove empty buffer sizes
        self.buffers.retain(|_, buffers| !buffers.is_empty());
    }
}

#[allow(dead_code)]
enum SyncChunkSource {
    Texture {
        texture: Texture,
        layout: TexelCopyBufferLayout,
        size: Extent3d,
    },
    Buffer {
        src_start: u64,
        dst_start: u64,
        buffer: Buffer,
    },
}

#[derive(Resource, Default)]
struct GpuSyncChunks {
    requested_download: Vec<GpuSyncChunkDownload>,
    mapped_download: Vec<GpuSyncChunkDownload>,
}

struct GpuSyncChunkDownload {
    pub entity: Entity,
    pub layer_index: u32,
    pub src: SyncChunkSource,
    pub buffer: Buffer,
    pub rx: Receiver<(Entity, Buffer, Vec<u8>)>,
    pub tx: Sender<(Entity, Buffer, Vec<u8>)>,
}

fn sync_readbacks(
    mut main_world: ResMut<MainWorld>,
    mut buffer_pool: ResMut<GpuSyncChunkBufferPool>,
    mut image_copiers: ResMut<ImageCopiers>,
    max_unused_frames: Res<GpuSyncChunkMaxUnusedFrames>,
) {
    image_copiers.mapped.retain(|readback| {
        if let Ok((entity, buffer, result)) = readback.rx.try_recv() {
            main_world.trigger_targets(
                SyncChunkComplete {
                    buffer,
                    data: result,
                },
                entity,
            );
            // buffer_pool.return_buffer(&buffer);
            false
        } else {
            true
        }
    });

    // buffer_pool.update(max_unused_frames.0);
}

fn texture_copy_extract(
    render_device: Res<RenderDevice>,
    mut readbacks: ResMut<GpuSyncChunks>,
    mut buffer_pool: ResMut<GpuSyncChunkBufferPool>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    handles: Query<(&MainEntity, &SyncChunk)>,
    mut commands: Commands,
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

// Utils

/// Round up a given value to be a multiple of [`COPY_BYTES_PER_ROW_ALIGNMENT`].
pub(crate) const fn align_byte_size(value: u32) -> u32 {
    RenderDevice::align_copy_bytes_per_row(value as usize) as u32
}

/// Get the size of a image when the size of each row has been rounded up to [`COPY_BYTES_PER_ROW_ALIGNMENT`].
pub(crate) const fn get_aligned_size(extent: Extent3d, pixel_size: u32) -> u32 {
    extent.height * align_byte_size(extent.width * pixel_size) * extent.depth_or_array_layers
}

/// Get a [`TexelCopyBufferLayout`] aligned such that the image can be copied into a buffer.
pub(crate) fn layout_data(extent: Extent3d, format: TextureFormat) -> TexelCopyBufferLayout {
    TexelCopyBufferLayout {
        bytes_per_row: if extent.height > 1 || extent.depth_or_array_layers > 1 {
            // 1 = 1 row
            Some(get_aligned_size(
                Extent3d {
                    width: extent.width,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                format.pixel_size() as u32,
            ))
        } else {
            None
        },
        rows_per_image: if extent.depth_or_array_layers > 1 {
            let (_, block_dimension_y) = format.block_dimensions();
            Some(extent.height / block_dimension_y)
        } else {
            None
        },
        offset: 0,
    }
}
