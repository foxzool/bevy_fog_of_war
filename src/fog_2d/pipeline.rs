use crate::fog_2d::chunk::{ChunkCoord, CHUNK_SIZE};
use crate::{FogOfWarScreen, FogOfWarSettings, FOG_OF_WAR_2D_SHADER_HANDLE};
use bevy::render::renderer::RenderQueue;
use bevy::{
    prelude::{FromWorld, Resource, World},
    render::{
        mesh::{PrimitiveTopology, VertexBufferLayout},
        render_resource::{
            binding_types::texture_storage_2d_array,
            binding_types::{storage_buffer_read_only_sized, uniform_buffer},
            BindGroupLayout, BindGroupLayoutEntries, BlendComponent, BlendState, Buffer,
            BufferAddress, BufferInitDescriptor, BufferUsages, CachedRenderPipelineId,
            ColorTargetState, ColorWrites, Extent3d, FragmentState, FrontFace, MultisampleState,
            PipelineCache, PolygonMode, PrimitiveState, RenderPipelineDescriptor, ShaderStages,
            StorageTextureAccess, TextureDescriptor, TextureDimension, TextureFormat,
            TextureUsages, TextureView, TextureViewDescriptor, VertexAttribute, VertexFormat,
            VertexState, VertexStepMode,
        },
        renderer::RenderDevice,
    },
};
use bevy::prelude::{DetectChanges, Res, ResMut};

#[derive(Resource)]
pub struct FogOfWar2dPipeline {
    pub bind_group_layout: BindGroupLayout,
    pub pipeline_id: CachedRenderPipelineId,
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub explored_texture: Option<TextureView>,
    pub texture: Option<bevy::render::render_resource::Texture>,
}

impl FromWorld for FogOfWar2dPipeline {
    fn from_world(world: &mut World) -> Self {
        // 计算基于屏幕大小的最大可见chunks
        let screen = world.resource::<FogOfWarScreen>();
        let (chunks_x, chunks_y) = screen.calculate_max_chunks();
        let render_device = world.resource_mut::<RenderDevice>();


        
        // 为了实现环形缓存，我们需要比实际视口多2行2列的chunks
        // 这样在相机移动时可以预先加载新的chunks
        let texture_array_size = ((chunks_x + 2) * (chunks_y + 2)) as u32;

        let texture = render_device.create_texture(&TextureDescriptor {
            label: Some("fog_explored_texture"),
            size: Extent3d {
                width: CHUNK_SIZE as u32,
                height: CHUNK_SIZE as u32,
                depth_or_array_layers: texture_array_size,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::COPY_SRC
                | TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });

        let explored_texture = texture.create_view(&TextureViewDescriptor {
            dimension: Some(bevy::render::render_resource::TextureViewDimension::D2Array),
            ..TextureViewDescriptor::default()
        });

        let bind_group_layout = render_device.create_bind_group_layout(
            "fog_of_war_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    uniform_buffer::<FogOfWarSettings>(true),
                    storage_buffer_read_only_sized(false, None),
                    texture_storage_2d_array(
                        TextureFormat::R8Unorm,
                        StorageTextureAccess::ReadWrite,
                    ),
                    uniform_buffer::<FogOfWarScreen>(false),
                ),
            ),
        );

        let vertex_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: BufferUsages::VERTEX,
        });
        let index_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: BufferUsages::INDEX,
        });

        let pipeline_id = world.resource_mut::<PipelineCache>().queue_render_pipeline(
            RenderPipelineDescriptor {
                label: Some("fog_of_war_2d_pipeline".into()),
                layout: vec![bind_group_layout.clone()],
                vertex: VertexState {
                    shader: FOG_OF_WAR_2D_SHADER_HANDLE,
                    entry_point: "vs_main".into(),
                    buffers: vec![Vertex::desc()],
                    shader_defs: vec![],
                },
                fragment: Some(FragmentState {
                    shader: FOG_OF_WAR_2D_SHADER_HANDLE,
                    shader_defs: vec![],
                    entry_point: "fs_main".into(),
                    targets: vec![Some(ColorTargetState {
                        format: TextureFormat::Rgba8UnormSrgb, // 明确指定格式
                        blend: Some(BlendState {
                            color: BlendComponent {
                                src_factor: bevy::render::render_resource::BlendFactor::SrcAlpha,
                                dst_factor:
                                    bevy::render::render_resource::BlendFactor::OneMinusSrcAlpha,
                                operation: bevy::render::render_resource::BlendOperation::Add,
                            },
                            alpha: BlendComponent {
                                src_factor: bevy::render::render_resource::BlendFactor::SrcAlpha,
                                dst_factor:
                                    bevy::render::render_resource::BlendFactor::OneMinusSrcAlpha,
                                operation: bevy::render::render_resource::BlendOperation::Add,
                            },
                        }),
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                primitive: PrimitiveState {
                    topology: PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                push_constant_ranges: vec![],
                zero_initialize_workgroup_memory: false,
            },
        );

        Self {
            bind_group_layout,
            pipeline_id,
            vertex_buffer,
            index_buffer,
            explored_texture: Some(explored_texture),
            texture: Some(texture),
        }
    }
}

impl FogOfWar2dPipeline {
    pub fn clear_explored_texture(&self, queue: &RenderQueue, chunk_index: i32) {
        if let Some(texture) = &self.texture {
            // 创建一个全零的缓冲区，大小为一个chunk的大小
            let zeros = vec![0u8; (CHUNK_SIZE * CHUNK_SIZE) as usize];

            // 写入数据到纹理的指定层
            queue.write_texture(
                bevy::render::render_resource::ImageCopyTexture {
                    texture,
                    mip_level: 0,
                    origin: bevy::render::render_resource::Origin3d {
                        x: 0,
                        y: 0,
                        z: chunk_index as u32,
                    },
                    aspect: bevy::render::render_resource::TextureAspect::All,
                },
                &zeros,
                bevy::render::render_resource::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(CHUNK_SIZE as u32),
                    rows_per_image: Some(CHUNK_SIZE as u32),
                },
                Extent3d {
                    width: CHUNK_SIZE as u32,
                    height: CHUNK_SIZE as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    pub fn transfer_chunk_data(
        &self,
        device: &RenderDevice,
        queue: &RenderQueue,
        from_index: i32,
        to_index: i32,
    ) {
        if let Some(texture) = &self.texture {
            // 创建一个命令编码器
            let mut encoder = device.create_command_encoder(
                &bevy::render::render_resource::CommandEncoderDescriptor {
                    label: Some("transfer_chunk_data_encoder"),
                },
            );

            // println!("from_index: {}, to_index: {}", from_index, to_index);

            // 使用copy_texture_to_texture在同一纹理的不同层之间复制数据
            encoder.copy_texture_to_texture(
                bevy::render::render_resource::ImageCopyTexture {
                    texture,
                    mip_level: 0,
                    origin: bevy::render::render_resource::Origin3d {
                        x: 0,
                        y: 0,
                        z: from_index as u32,
                    },
                    aspect: bevy::render::render_resource::TextureAspect::All,
                },
                bevy::render::render_resource::ImageCopyTexture {
                    texture,
                    mip_level: 0,
                    origin: bevy::render::render_resource::Origin3d {
                        x: 0,
                        y: 0,
                        z: to_index as u32,
                    },
                    aspect: bevy::render::render_resource::TextureAspect::All,
                },
                Extent3d {
                    width: CHUNK_SIZE as u32,
                    height: CHUNK_SIZE as u32,
                    depth_or_array_layers: 1,
                },
            );

            // 提交命令
            queue.submit(std::iter::once(encoder.finish()));

            // 清空源chunk
            self.clear_explored_texture(queue, from_index);
        }
    }

    pub fn recreate_texture(&mut self, device: &RenderDevice, screen: &FogOfWarScreen) {
        // 计算基于屏幕大小的纹理数组大小
        let (chunks_x, chunks_y) = screen.calculate_max_chunks();
        let texture_array_size = ((chunks_x + 2) * (chunks_y + 2)) as u32;

        // 创建新的纹理
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("fog_explored_texture"),
            size: Extent3d {
                width: CHUNK_SIZE as u32,
                height: CHUNK_SIZE as u32,
                depth_or_array_layers: texture_array_size,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::COPY_SRC
                | TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });

        let explored_texture = texture.create_view(&TextureViewDescriptor {
            dimension: Some(bevy::render::render_resource::TextureViewDimension::D2Array),
            ..TextureViewDescriptor::default()
        });

        self.explored_texture = Some(explored_texture);
        self.texture = Some(texture);
    }
}

// 添加一个系统来处理窗口大小变化
pub fn handle_screen_resize(
    screen: Res<FogOfWarScreen>,
    device: Res<RenderDevice>,
    mut pipeline: ResMut<FogOfWar2dPipeline>,
) {
    if screen.is_changed() {
        pipeline.recreate_texture(&device, &screen);
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
}

impl Vertex {
    fn desc() -> VertexBufferLayout {
        VertexBufferLayout {
            array_stride: size_of::<Vertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: vec![
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: size_of::<[f32; 3]>() as BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x4,
                },
            ],
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-1.0, -1.0, 0.0], // 左下
        color: [0.0, 0.0, 0.0, 1.0],
    },
    Vertex {
        position: [1.0, -1.0, 0.0], // 右下
        color: [0.0, 0.0, 0.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0, 0.0], // 右上
        color: [0.0, 0.0, 0.0, 1.0],
    },
    Vertex {
        position: [-1.0, 1.0, 0.0], // 左上
        color: [0.0, 0.0, 0.0, 1.0],
    },
];

const INDICES: &[u16] = &[0, 1, 2, 0, 2, 3]; // 两个三角形组成一个矩形
