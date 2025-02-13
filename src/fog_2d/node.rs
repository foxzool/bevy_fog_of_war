use crate::fog_2d::buffers::{FogOfWarRingBuffers, FogOfWarSettingBuffer};
use crate::{
    fog_2d::buffers::FogSight2dBuffers, fog_2d::pipeline::FogOfWar2dPipeline, FogOfWarSettings,
};
use bevy::ecs::system::lifetimeless::Read;
use bevy::{
    ecs::query::QueryItem,
    prelude::{default, World},
    render::{
        extract_component::{ComponentUniforms, DynamicUniformIndex},
        render_graph::{NodeRunError, RenderGraphContext, RenderLabel, ViewNode},
        render_resource::{
            BindGroupEntries, IndexFormat, IntoBinding, LoadOp, Operations, PipelineCache,
            RenderPassColorAttachment, RenderPassDescriptor, StoreOp,
        },
        renderer::RenderContext,
        view::ViewTarget,
    },
};
use bevy_render::view::{ViewUniformOffset, ViewUniforms};

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct FogOfWarLabel;

#[derive(Default)]
pub struct FogOfWar2dNode;

impl ViewNode for FogOfWar2dNode {
    type ViewQuery = (&'static ViewTarget, Read<ViewUniformOffset>);

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, view_uniform_offset): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let fog_of_war_pipeline = world.resource::<FogOfWar2dPipeline>();
        let view_uniforms = world.resource::<ViewUniforms>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let fog_sight_buffers = world.resource::<FogSight2dBuffers>();
        let ring_buffers = world.resource::<FogOfWarRingBuffers>();

        let Some(view_uniforms_binding) = view_uniforms.uniforms.binding() else {
            return Ok(());
        };

        let Some(pipeline) = pipeline_cache.get_render_pipeline(fog_of_war_pipeline.pipeline_id)
        else {
            return Ok(());
        };

        let Some(settings_binding) = world.resource::<FogOfWarSettingBuffer>().buffer.binding()
        else {
            return Ok(());
        };

        let view = view_target.main_texture_view();

        let bind_group = render_context.render_device().create_bind_group(
            None,
            &fog_of_war_pipeline.bind_group_layout,
            &BindGroupEntries::sequential((
                view_uniforms_binding,
                settings_binding.clone(),
                fog_sight_buffers.buffers.into_binding(),
                fog_of_war_pipeline.explored_texture.as_ref().unwrap(),
                ring_buffers.buffers.into_binding(),
            )),
        );

        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("fog_of_war_2d_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            ..default()
        });

        render_pass.set_render_pipeline(pipeline);
        render_pass.set_bind_group(0, &bind_group, &[view_uniform_offset.offset]);
        render_pass.set_vertex_buffer(0, fog_of_war_pipeline.vertex_buffer.slice(..));
        render_pass.set_index_buffer(
            fog_of_war_pipeline.index_buffer.slice(..),
            0,
            IndexFormat::Uint16,
        );

        render_pass.draw_indexed(0..6, 0, 0..1);

        Ok(())
    }
}
