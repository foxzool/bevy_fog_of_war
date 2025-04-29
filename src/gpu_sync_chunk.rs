use crate::chunk::ChunkManager;
use crate::prelude::ChunkCoord;
use crate::vision_compute::ExploredTexture;
use bevy_app::{App, Plugin};
use bevy_asset::Handle;
use bevy_core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy_diagnostic::FrameCount;
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs, Query, Res, Resource, World};
use bevy_ecs::system::ResMut;
use bevy_image::{Image, TextureFormatPixelInfo};
use bevy_render::render_asset::RenderAssets;
use bevy_render::render_graph::{
    NodeRunError, RenderGraphApp, RenderGraphContext, ViewNode, ViewNodeRunner,
};
use bevy_render::render_resource::{
    Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, Maintain, MapMode,
    Origin3d, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
};
use bevy_render::renderer::{RenderContext, RenderDevice, RenderQueue, render_system};
use bevy_render::texture::GpuImage;
use bevy_render::{Extract, ExtractSchedule, Render, RenderApp, RenderSet};
use bevy_render_macros::{ExtractComponent, RenderLabel};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A plugin that enables reading back gpu buffers and textures to the cpu.
pub struct GpuSyncChunkPlugin {
    /// Describes the number of frames a buffer can be unused before it is removed from the pool in
    /// order to avoid unnecessary reallocations.
    max_unused_frames: usize,
}

impl Default for GpuSyncChunkPlugin {
    fn default() -> Self {
        Self {
            max_unused_frames: 10,
        }
    }
}

impl Plugin for GpuSyncChunkPlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .init_resource::<ImageCopiers>()
            .add_systems(ExtractSchedule, image_copy_extract)
            .add_systems(
                Render,
                map_buffers.after(render_system).in_set(RenderSet::Render),
            );

        // render_app.add_systems(Render, log_start.in_set(RenderSet::ExtractCommands));
        // render_app.add_systems(Render, log_end.in_set(RenderSet::PostCleanup));

        render_app
            .add_render_graph_node::<ViewNodeRunner<ExploredSyncNode>>(
                Core2d,
                ExploredTextureLoader,
            )
            .add_render_graph_edges(
                Core2d,
                (
                    Node2d::MainTransparentPass,
                    ExploredTextureLoader,
                    Node2d::EndMainPass,
                ),
            );
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, RenderLabel)]
pub struct ExploredTextureLoader;

/// `RenderGraph` node
#[derive(Default)]
struct ExploredSyncNode;

impl ViewNode for ExploredSyncNode {
    type ViewQuery = ();

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (): Self::ViewQuery,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let image_copiers = world.get_resource::<ImageCopiers>().unwrap();
        let gpu_images = world.get_resource::<RenderAssets<GpuImage>>().unwrap();
        let explored_textures = world.get_resource::<ExploredTexture>().unwrap();
        let render_device = world.get_resource::<RenderDevice>().unwrap();
        let (Some(explored_read), Some(explored_write)) =
            (&explored_textures.read, &explored_textures.write)
        else {
            return Ok(());
        };
        if !image_copiers.clean.is_empty() {
            // println!("download {} chunks", image_copiers.uploader.len());
        }
        // download layer image
        for image_copier in image_copiers.download.iter() {
            if !image_copier.enabled() {
                continue;
            }

            let src_image = gpu_images.get(&image_copier.src_image).unwrap();

            let mut encoder = render_context
                .render_device()
                .create_command_encoder(&CommandEncoderDescriptor::default());

            let block_dimensions = src_image.texture_format.block_dimensions();
            let block_size = src_image.texture_format.block_copy_size(None).unwrap();

            let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(
                (src_image.size.width as usize / block_dimensions.0 as usize) * block_size as usize,
            );

            encoder.copy_texture_to_buffer(
                TexelCopyTextureInfo {
                    texture: &explored_write.texture,
                    mip_level: 0,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: image_copier.layer_index.unwrap(),
                    },
                    aspect: TextureAspect::All,
                },
                TexelCopyBufferInfo {
                    buffer: &image_copier.buffer,
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

            let render_queue = world.get_resource::<RenderQueue>().unwrap();
            render_queue.submit(std::iter::once(encoder.finish()));
        }

        if !image_copiers.clean.is_empty() {
            // println!("remove {} chunks", image_copiers.uploader.len());
        }
        // remove layer image
        for image_copier in image_copiers.clean.iter() {
            if !image_copier.enabled() {
                continue;
            }

            let src_image = gpu_images.get(&image_copier.src_image).unwrap();

            let mut encoder = render_context
                .render_device()
                .create_command_encoder(&CommandEncoderDescriptor::default());
            let block_dimensions = src_image.texture_format.block_dimensions();
            let block_size = src_image.texture_format.block_copy_size(None).unwrap();

            let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(
                (src_image.size.width as usize / block_dimensions.0 as usize) * block_size as usize,
            );

            let width = src_image.size.width;
            let height = src_image.size.height;

            // Calculate bytes per row for the clear buffer based on raw width and pixel size (assuming R8Uint here, adjust if needed)
            // NOTE: This assumes the format is uncompressed or you want to clear based on raw pixel dimensions.
            // If the format IS compressed, clearing might need a different approach or ensure the buffer matches the compressed block structure.
            // Let's assume for now the intent is to clear raw pixels. We need the actual bytes per pixel.
            // For simplicity, let's stick to the original calculation but use the CORRECT variable in the layout.
            // Revisit this if the format is complex (e.g., compressed).
            let clear_bytes_per_row_unpadded =
                width as usize * src_image.texture_format.pixel_size(); // Use pixel_size() for correctness
            let clear_padded_bytes_per_row =
                RenderDevice::align_copy_bytes_per_row(clear_bytes_per_row_unpadded);
            let clear_buffer_size = clear_padded_bytes_per_row * height as usize;

            let zero_data = vec![0u8; clear_buffer_size];
            let buffer = render_device.create_buffer_with_data(
                &bevy_render::render_resource::BufferInitDescriptor {
                    label: Some("clear_explored_layer_buffer"),
                    contents: &zero_data,
                    usage: BufferUsages::COPY_SRC,
                },
            );

            encoder.copy_buffer_to_texture(
                TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: TexelCopyBufferLayout {
                        offset: 0,
                        // ***** CHANGE HERE *****
                        // Use the bytes_per_row calculated for the zero_data buffer
                        bytes_per_row: Some(u32::from(
                            std::num::NonZeroU32::new(clear_padded_bytes_per_row as u32)
                                .expect("Clear buffer row size should not be zero"),
                        )),
                        // rows_per_image should likely be None when copying to a single 2D layer/slice
                        rows_per_image: None,
                    },
                },
                TexelCopyTextureInfo {
                    // Target the correct texture (explored_read seems right for clearing render data)
                    texture: &explored_read.texture,
                    mip_level: 0,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: image_copier.layer_index.unwrap(),
                    },
                    aspect: TextureAspect::All,
                },
                Extent3d {
                    // Ensure the extent matches the area being cleared
                    width,
                    height,
                    depth_or_array_layers: 1, // Clearing one layer
                },
            );
            // --- End Clear texture operation ---

            // println!("download {:?}", image_copier.chunk_coord); // Keep this commented if not needed

            let render_queue = world.get_resource::<RenderQueue>().unwrap();
            render_queue.submit(std::iter::once(encoder.finish()));
        }

        // upload layer image
        if !image_copiers.uploader.is_empty() {
            // println!("upload {} chunks", image_copiers.uploader.len());
        }

        for image_copier in image_copiers.uploader.iter() {
            if !image_copier.enabled() {
                continue;
            }

            let src_image = gpu_images.get(&image_copier.src_image).unwrap();

            let mut encoder = render_context
                .render_device()
                .create_command_encoder(&CommandEncoderDescriptor::default());

            let block_dimensions = src_image.texture_format.block_dimensions();
            let block_size = src_image.texture_format.block_copy_size(None).unwrap();

            let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(
                (src_image.size.width as usize / block_dimensions.0 as usize) * block_size as usize,
            );

            encoder.copy_buffer_to_texture(
                TexelCopyBufferInfo {
                    buffer: &image_copier.buffer,
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
                        z: image_copier.layer_index.unwrap(),
                    },
                    aspect: TextureAspect::All,
                },
                src_image.size,
            );

            let render_queue = world.get_resource::<RenderQueue>().unwrap();
            render_queue.submit(std::iter::once(encoder.finish()));
        }

        Ok(())
    }
}

fn image_copy_extract(
    mut commands: Commands,
    image_copiers: Extract<Query<&ImageCopier>>,
    chunk_manager: Extract<Res<ChunkManager>>,
    frame_count: Extract<Res<FrameCount>>,
) {
    let mut download = vec![];
    let mut uploader = vec![];
    let mut clean = vec![];
    for (chunk, layer_index) in chunk_manager.sync_to_world.iter() {
        for image_copier in image_copiers.iter() {
            if image_copier.chunk_coord == *chunk {
                download.push(ImageCopier {
                    layer_index: Some(*layer_index),
                    ..image_copier.clone()
                });
            }
        }
    }
    for (chunk, layer_index) in chunk_manager.sync_to_render.iter() {
        for image_copier in image_copiers.iter() {
            if image_copier.chunk_coord == *chunk {
                uploader.push(ImageCopier {
                    layer_index: Some(*layer_index),
                    upload: true,
                    ..image_copier.clone()
                });
            }
        }
    }
    for (chunk, layer_index) in chunk_manager.sync_to_clean.iter() {
        for image_copier in image_copiers.iter() {
            if image_copier.chunk_coord == *chunk {
                clean.push(ImageCopier {
                    layer_index: Some(*layer_index),
                    ..image_copier.clone()
                });
            }
        }
    }

    commands.insert_resource(ImageCopiers {
        uploader,
        download,
        clean,
    });
}

/// `ImageCopier` aggregator in `RenderWorld`
#[derive(Clone, Default, Resource)]
struct ImageCopiers {
    uploader: Vec<ImageCopier>,
    download: Vec<ImageCopier>,
    clean: Vec<ImageCopier>,
}

/// Used by `ImageCopyDriver` for copying from render target to buffer
#[derive(Clone, Component, ExtractComponent, Debug)]
pub struct ImageCopier {
    buffer: Buffer,
    enabled: Arc<AtomicBool>,
    src_image: Handle<Image>,
    chunk_coord: ChunkCoord,
    layer_index: Option<u32>,
    upload: bool,
}

impl ImageCopier {
    pub fn new(
        chunk_coord: ChunkCoord,
        src_image: Handle<Image>,
        size: Extent3d,
        render_device: &RenderDevice,
    ) -> ImageCopier {
        let padded_bytes_per_row =
            RenderDevice::align_copy_bytes_per_row((size.width) as usize) * 4;

        let cpu_buffer = render_device.create_buffer(&BufferDescriptor {
            label: None,
            size: padded_bytes_per_row as u64 * size.height as u64,
            usage: BufferUsages::MAP_READ
                | BufferUsages::MAP_WRITE
                | BufferUsages::COPY_DST
                | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        ImageCopier {
            chunk_coord,
            buffer: cpu_buffer,
            src_image,
            enabled: Arc::new(AtomicBool::new(true)),
            layer_index: None,
            upload: false,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

fn map_buffers(mut readbacks: ResMut<ImageCopiers>, render_device: Res<RenderDevice>) {
    let requested = readbacks.uploader.drain(..).collect::<Vec<ImageCopier>>();
    for readback in requested {
        let slice = readback.buffer.slice(..);

        let buffer = readback.buffer.clone();
        slice.map_async(MapMode::Read, move |res| {
            res.expect("Failed to map buffer");
            buffer.unmap();
        });

        render_device.poll(Maintain::wait()).panic_on_timeout();
    }
}
