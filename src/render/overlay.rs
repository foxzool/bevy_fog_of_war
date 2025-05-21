// fog_render/overlay.rs
use bevy::core_pipeline::fullscreen_vertex_shader::FULLSCREEN_SHADER_HANDLE;
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::lifetimeless::Read;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{NodeRunError, RenderGraphContext, RenderLabel, ViewNode};
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only, texture_2d_array, uniform_buffer,
};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::render::view::{ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms};
use crate::snapshot::SnapshotCamera;
use super::RenderFogMapSettings;
use super::extract::{
    OverlayChunkData, RenderFogTexture, RenderSnapshotTexture, RenderVisibilityTexture,
};
use super::prepare::{FogUniforms, OverlayChunkMappingBuffer};

const SHADER_ASSET_PATH: &str = "shaders/fog_overlay.wgsl";

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

impl FromWorld for FogOverlayPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();

        let layout = render_device.create_bind_group_layout(
            "fog_overlay_bind_group_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    uniform_buffer::<ViewUniform>(true),    // 0
                    sampler(SamplerBindingType::Filtering), // 1
                    texture_2d_array(TextureSampleType::Float { filterable: true }), // 2
                    texture_2d_array(TextureSampleType::Float { filterable: true }), // 3
                    texture_2d_array(TextureSampleType::Float { filterable: true }), // 4
                    uniform_buffer::<RenderFogMapSettings>(false), // 5
                    storage_buffer_read_only::<OverlayChunkData>(false), // 6
                ),
            ),
        );
        // Create a sampler / 创建采样器
        let sampler = render_device.create_sampler(&SamplerDescriptor {
            label: Some("fog_overlay_sampler"),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            ..Default::default()
        });

        let shader = world.load_asset(SHADER_ASSET_PATH);
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
                        shader,
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

impl ViewNode for FogOverlayNode {
    type ViewQuery = (Read<ViewTarget>, Read<ViewUniformOffset>);

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, view_uniform_offset): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let view_entity = graph.view_entity();

        if world.get::<SnapshotCamera>(view_entity).is_some() {
            return Ok(());
        }
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
        let visibility_texture = world.resource::<RenderVisibilityTexture>();
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
        let visibility_texture_view = images
            .get(&visibility_texture.0)
            .map(|img| &img.texture_view)
            .unwrap_or(&fallback_image.d2.texture_view);

        let fog_texture_view = images
            .get(&fog_texture.0)
            .map(|img| &img.texture_view)
            .unwrap_or(&fallback_image.d2.texture_view);

        let snapshot_texture_view = images
            .get(&snapshot_texture.0)
            .map(|img| &img.texture_view)
            .unwrap_or(&fallback_image.d2.texture_view);

        // Create the bind group for this specific view / 为此特定视图创建绑定组
        let bind_group = render_context.render_device().create_bind_group(
            "fog_overlay_bind_group",
            &overlay_pipeline.layout,
            &BindGroupEntries::sequential((
                view_uniform_binding,            // 0
                &overlay_pipeline.sampler,       // 1
                visibility_texture_view,         // 2
                fog_texture_view,                // 3
                snapshot_texture_view,           // 4
                uniform_buf.as_entire_binding(), // 5
                mapping_buf.as_entire_binding(), // 6
            )),
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
