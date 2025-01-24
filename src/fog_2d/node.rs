use crate::{
    fog_2d::buffers::FogSight2dBuffers, fog_2d::buffers::FogSight2dScreenBuffers,
    fog_2d::pipeline::FogOfWar2dPipeline, FogOfWarScreen, FogOfWarSettings,
};
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

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct FogOfWarLabel;

#[derive(Default)]
pub struct FogOfWar2dNode;

impl ViewNode for FogOfWar2dNode {
    type ViewQuery = (
        &'static ViewTarget,
        &'static FogOfWarSettings,
        &'static DynamicUniformIndex<FogOfWarSettings>,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, _fog_of_war_settings, settings_index): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let fog_of_war_pipeline = world.resource::<FogOfWar2dPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let fog_sight_buffers = world.resource::<FogSight2dBuffers>();

        let Some(pipeline) = pipeline_cache.get_render_pipeline(fog_of_war_pipeline.pipeline_id)
        else {
            return Ok(());
        };

        let settings_uniforms = world.resource::<ComponentUniforms<FogOfWarSettings>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
            return Ok(());
        };

        let screen_uniform = world.resource::<FogSight2dScreenBuffers>();

        let view = view_target.main_texture_view();

        let bind_group = render_context.render_device().create_bind_group(
            None,
            &fog_of_war_pipeline.bind_group_layout,
            &BindGroupEntries::sequential((
                settings_binding.clone(),
                fog_sight_buffers.buffers.into_binding(),
                fog_of_war_pipeline.explored_texture.as_ref().unwrap(),
                &screen_uniform.buffers,
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
        render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
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
