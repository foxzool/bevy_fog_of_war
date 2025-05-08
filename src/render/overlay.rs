// fog_render/overlay.rs
use bevy::core_pipeline::fullscreen_vertex_shader::{
    FULLSCREEN_SHADER_HANDLE, fullscreen_shader_vertex_state,
};
use bevy::ecs::query::QueryItem;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{NodeRunError, RenderGraphContext, RenderLabel, ViewNode};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::render::view::{ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms};
// Import ViewUniform / 导入 ViewUniform // For default texture / 用于默认纹理

use super::extract::{OverlayChunkData, RenderFogTexture, RenderSnapshotTexture};
use super::prepare::{FogBindGroups, FogUniforms, OverlayChunkMappingBuffer};
use super::{FOG_OVERLAY_SHADER_HANDLE, RenderFogMapSettings};

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct FogOverlayNodeLabel;

#[derive(Default)]
pub struct FogOverlayNode;

// Pipeline resource for the overlay shader / 覆盖 shader 的管线资源
#[derive(Resource)]
pub struct FogOverlayPipeline {
    layout: BindGroupLayout,
    sampler: Sampler, // Store sampler / 存储采样器
    pipeline_id: CachedRenderPipelineId,
}

impl SpecializedRenderPipeline for FogOverlayPipeline {
    type Key = (); // No specialization needed for this simple overlay / 这个简单的覆盖不需要特化

    fn specialize(&self, _key: Self::Key) -> RenderPipelineDescriptor {
        let layout = vec![self.layout.clone()];
        // Use fullscreen vertex shader and custom fragment shader / 使用全屏顶点着色器和自定义片段着色器
        RenderPipelineDescriptor {
            label: Some("fog_overlay_pipeline".into()),
            layout,
            vertex: fullscreen_shader_vertex_state(),
            fragment: Some(FragmentState {
                shader: FOG_OVERLAY_SHADER_HANDLE, // Our custom fragment shader / 我们的自定义片段着色器
                shader_defs: vec![],
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: ViewTarget::TEXTURE_FORMAT_HDR, // Target HDR format / 目标 HDR 格式
                    blend: Some(BlendState::ALPHA_BLENDING), // Enable alpha blending / 启用 alpha 混合
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(), // Default triangle list / 默认三角形列表
            depth_stencil: None,                  // No depth testing/writing / 无深度测试/写入
            multisample: MultisampleState::default(), // No MSAA / 无 MSAA
            push_constant_ranges: vec![],
            zero_initialize_workgroup_memory: false,
        }
    }
}

impl FromWorld for FogOverlayPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();

        let layout = render_device.create_bind_group_layout(
            "fog_overlay_bind_group_layout",
            &[
                // View Uniforms (Standard Bevy Binding) / 视图统一变量 (标准 Bevy 绑定)
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true, // Important for view uniforms / 对视图统一变量很重要
                        min_binding_size: Some(ViewUniform::min_size()),
                    },
                    count: None,
                },
                // Fog Texture (Sampled) / 雾效纹理 (采样)
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false }, // R8Unorm is not filterable / R8Unorm 不可过滤
                        view_dimension: TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // Snapshot Texture (Sampled) / 快照纹理 (采样)
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true }, // RGBA8 is filterable / RGBA8 可过滤
                        view_dimension: TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler / 采样器
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::NonFiltering), // Use filtering for snapshot / 对快照使用过滤
                    count: None,
                },
                // Fog Settings (Uniform Buffer) / 雾设置 (统一缓冲区) - Reuse binding 3 from compute layout? No, use new binding.
                // 重用计算布局中的绑定 3？不，使用新绑定。
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::FRAGMENT, // Only fragment needed here / 这里只需要片段
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(RenderFogMapSettings::min_size()),
                    },
                    count: None,
                },
                // Overlay Chunk Mapping (Storage Buffer) / 覆盖区块映射 (存储缓冲区)
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(OverlayChunkData::min_size()),
                    },
                    count: None,
                },
            ],
        );

        // Create a sampler / 创建采样器
        let sampler = render_device.create_sampler(&SamplerDescriptor {
            label: Some("fog_overlay_sampler"),
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest, // No mipmaps / 无 mipmap
            address_mode_u: AddressMode::ClampToEdge, // Clamp coordinates / 夹紧坐标
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            ..Default::default()
        });

        let pipeline_id =
            world
                .resource_mut::<PipelineCache>()
                .queue_render_pipeline(RenderPipelineDescriptor {
                    label: Some("fog_overlay_pipeline_init".into()), // Initial descriptor label / 初始描述符标签
                    layout: vec![layout.clone()],
                    vertex: VertexState {
                        shader: FULLSCREEN_SHADER_HANDLE,
                        shader_defs: vec![],
                        entry_point: "fullscreen_vertex_shader".into(),
                        buffers: vec![],
                    },
                    fragment: Some(FragmentState {
                        shader: FOG_OVERLAY_SHADER_HANDLE,
                        shader_defs: vec![],
                        entry_point: "fragment".into(),
                        targets: vec![Some(ColorTargetState {
                            format: TextureFormat::bevy_default(),
                            blend: Some(BlendState::ALPHA_BLENDING),
                            write_mask: ColorWrites::ALL,
                        })],
                    }),
                    primitive: PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: MultisampleState::default(),
                    push_constant_ranges: vec![],
                    zero_initialize_workgroup_memory: false,
                });

        FogOverlayPipeline {
            layout,
            sampler,
            pipeline_id,
        }
    }
}

// System to queue the specialized pipeline instance / 排队特化管线实例的系统
pub fn queue_fog_overlay_pipelines(
    mut pipeline_cache: ResMut<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<FogOverlayPipeline>>,
    pipeline: Res<FogOverlayPipeline>,
    views: Query<Entity, With<ViewTarget>>, // Queue for all views with a ViewTarget / 为所有带 ViewTarget 的视图排队
) {
    for view_entity in views.iter() {
        // Queue the pipeline for this view / 为此视图排队管线
        pipelines.specialize(&mut pipeline_cache, &pipeline, ());
    }
}

impl ViewNode for FogOverlayNode {
    // Query ViewTarget and ViewUniformOffset / 查询 ViewTarget 和 ViewUniformOffset
    type ViewQuery = (
        &'static ViewTarget,
        &'static ViewUniformOffset,
        &'static Msaa,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, view_uniform_offset, msaa): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let overlay_pipeline = world.resource::<FogOverlayPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();


        // Get the specialized pipeline for this view / 获取此视图的特化管线
        let Some(pipeline) = pipeline_cache.get_render_pipeline(overlay_pipeline.pipeline_id)
        else {
            // info!("Overlay pipeline not ready.");
            return Ok(());
        };

        // Get other needed resources / 获取其他所需资源
        let fog_uniforms = world.resource::<FogUniforms>();
        let overlay_chunk_buffer = world.resource::<OverlayChunkMappingBuffer>();
        let fog_texture = world.resource::<RenderFogTexture>();
        let snapshot_texture = world.resource::<RenderSnapshotTexture>();
        let images = world.resource::<RenderAssets<GpuImage>>();
        let fallback_image = world.resource::<FallbackImage>();
        let view_uniforms = world.resource::<ViewUniforms>();

        // Ensure buffers and view uniforms are ready / 确保缓冲区和视图统一变量已准备好
        let (
            Some(uniform_buf),
            Some(mapping_buf),
            Some(view_uniform_binding), // Get view uniform binding / 获取视图统一绑定
        ) = (
            fog_uniforms.buffer.as_ref(),
            overlay_chunk_buffer.buffer.as_ref(),
            view_uniforms.uniforms.binding(), // Get buffer binding / 获取缓冲区绑定
        )
        else {
            // info!("Overlay buffers or view uniforms not ready.");
            return Ok(());
        };

        // Get texture views / 获取纹理视图
        let fog_texture_view = images
            .get(&fog_texture.0)
            .map(|img| &img.texture_view)
            .unwrap_or(&fallback_image.d1.texture_view);

        let snapshot_texture_view = images
            .get(&snapshot_texture.0)
            .map(|img| &img.texture_view)
            .unwrap_or(&fallback_image.d1.texture_view);

        // Create the bind group for this specific view / 为此特定视图创建绑定组
        let bind_group = render_context.render_device().create_bind_group(
            "fog_overlay_bind_group",
            &overlay_pipeline.layout,
            &[
                BindGroupEntry {
                    binding: 0,
                    resource: view_uniform_binding,
                }, // View uniforms / 视图统一变量
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(fog_texture_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(snapshot_texture_view),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::Sampler(&overlay_pipeline.sampler),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: uniform_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: mapping_buf.as_entire_binding(),
                },
            ],
        );

        // Begin render pass targeting the view / 开始针对视图的渲染通道
        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("fog_overlay_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: view_target.main_texture_view(),
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_render_pipeline(pipeline);
        // Set bind group with dynamic offset for view uniforms / 设置带有视图统一变量动态偏移的绑定组
        render_pass.set_bind_group(0, &bind_group, &[view_uniform_offset.offset]);
        // Draw fullscreen quad (3 vertices, 1 instance) / 绘制全屏四边形 (3 个顶点, 1 个实例)
        render_pass.draw(0..3, 0..1);

        Ok(())
    }
}
