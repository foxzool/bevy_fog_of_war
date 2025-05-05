use bevy::ecs::query::QueryItem;
use bevy::pbr::{MeshPipeline, MeshPipelineKey};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{NodeRunError, RenderGraphContext, RenderLabel, ViewNode};
use bevy::render::render_phase::RenderCommand;
use bevy::render::render_resource::{BindGroupLayout, BindGroupLayoutDescriptor, LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, SpecializedMeshPipeline, SpecializedRenderPipeline, StoreOp, TextureViewDescriptor, TextureViewDimension};
use bevy::render::renderer::{RenderContext, RenderDevice};
// For depth / 用于深度

use super::extract::{RenderSnapshotTexture, SnapshotRequestQueue};
// Import the marker component / 导入标记组件

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct SnapshotNodeLabel;

#[derive(Default)]
pub struct SnapshotNode;

// Custom Render Phase for snapshot items / 快照项的自定义渲染阶段
#[derive(Component)]
pub struct SnapshotItem {
    // Add sorting data if needed / 如果需要添加排序数据
    pub entity: Entity,
    // pub distance: f32,
    // pub pipeline: CachedRenderPipelineId,
    // pub draw_function_id: DrawFunctionId,
}

// Pipeline for rendering snapshots (could be simple unlit or PBR)
// 用于渲染快照的管线 (可以是简单的无光照或 PBR)
#[derive(Resource)]
pub struct SnapshotPipeline {
    // Store necessary pipeline info / 存储必要的管线信息
    // Example: Using Bevy's MeshPipeline for simplicity
    // 示例: 为简单起见使用 Bevy 的 MeshPipeline
    mesh_pipeline: MeshPipeline,
    layout: BindGroupLayout, // Layout for snapshot-specific bindings if any / 快照特定绑定的布局 (如果有)
}

impl SpecializedRenderPipeline for SnapshotPipeline {
    type Key = MeshPipelineKey; // Use Mesh key if based on MeshPipeline / 如果基于 MeshPipeline 则使用 Mesh 键

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        // Specialize the pipeline based on key (e.g., PBR flags)
        // 基于键特化管线 (例如 PBR 标志)
        let mut descriptor = self.mesh_pipeline.specialize(key);
        // Modify descriptor if needed (e.g., different vertex/fragment shader, different layout)
        // 如果需要修改描述符 (例如，不同的顶点/片段着色器，不同的布局)
        descriptor.label = Some("snapshot_pipeline".into());
        // descriptor.layout = vec![self.layout.clone(), self.mesh_pipeline.get_layout().mesh_layout.clone()]; // Combine layouts / 组合布局
        descriptor
    }
}

impl FromWorld for SnapshotPipeline {
    fn from_world(world: &mut World) -> Self {
        // Get necessary resources like MeshPipeline, RenderDevice
        // 获取必要的资源，如 MeshPipeline, RenderDevice
        let mesh_pipeline = world.resource::<MeshPipeline>().clone();
        let render_device = world.resource::<RenderDevice>();
        // Create custom layout if needed / 如果需要创建自定义布局
        let layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("snapshot_layout"),
            entries: &[], // Add snapshot specific bindings here / 在此添加快照特定绑定
        });
        SnapshotPipeline {
            mesh_pipeline,
            layout,
        }
    }
}

// System to queue snapshot items into a RenderPhase
// 将快照项排队到 RenderPhase 的系统
pub fn queue_snapshot_pipelines(
// This system needs access to entities with Snapshottable
// It might need to run in the Extract schedule or have data extracted
// 此系统需要访问带有 Snapshottable 的实体
// 它可能需要在 Extract 调度中运行或提取数据
// draw_functions: Res<DrawFunctions<SnapshotItem>>, // Custom draw functions / 自定义绘制函数
// pipeline_cache: Res<PipelineCache>,
// mut pipelines: ResMut<SpecializedRenderPipelines<SnapshotPipeline>>,
// snapshot_pipeline: Res<SnapshotPipeline>,
// msaa: Res<Msaa>, // If using MSAA / 如果使用 MSAA
// render_meshes: Res<RenderAssets<Mesh>>,
// material_meta: Res<MaterialPipeline<YourMaterial>>, // If using custom material / 如果使用自定义材质
// snapshottable_query: Query<(Entity, &Handle<Mesh>, &MeshUniform, &Handle<YourMaterial>), With<Snapshottable>>,
// mut views: Query<&mut RenderPhase<SnapshotItem>>, // Query the custom phase / 查询自定义阶段
// snapshot_requests: Res<SnapshotRequestQueue>, // Get requests to know which chunks are active / 获取请求以了解哪些区块是活动的))
)
{
    // 1. Iterate through views (cameras) - though snapshotting might be view-independent
    //    遍历视图 (相机) - 尽管快照可能与视图无关
    // 2. For each active snapshot request (chunk):
    //    对于每个活动的快照请求 (区块):
    //    a. Iterate through all entities with `Snapshottable`.
    //       遍历所有带有 `Snapshottable` 的实体。
    //    b. Check if the entity's GlobalTransform is within the `request.world_bounds`.
    //       检查实体的 GlobalTransform 是否在 `request.world_bounds` 内。
    //    c. If inside, get mesh, material, pipeline key.
    //       如果在内部，获取网格、材质、管线键。
    //    d. Specialize the `SnapshotPipeline` using the key.
    //       使用键特化 `SnapshotPipeline`。
    //    e. Get the draw function ID.
    //       获取绘制函数 ID。
    //    f. Add a `SnapshotItem` to the `RenderPhase<SnapshotItem>` associated with a *conceptual* snapshot camera/view.
    //       将 `SnapshotItem` 添加到与 *概念性* 快照相机/视图关联的 `RenderPhase<SnapshotItem>`。
    // This queuing logic is complex because standard phases are tied to views.
    // 这个排队逻辑很复杂，因为标准阶段与视图绑定。
    // A simpler approach might be to render *all* snapshottables in the main pass
    // with a special marker/flag, then copy relevant parts in a compute shader,
    // but that's less efficient if only a few chunks need updates.
    // 一个更简单的方法可能是在主通道中渲染 *所有* snapshottables
    // 带有特殊标记/标志，然后在 compute shader 中复制相关部分，
    // 但如果只有少数区块需要更新，效率较低。
}

impl ViewNode for SnapshotNode {
    type ViewQuery = ();

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let snapshot_requests = world.resource::<SnapshotRequestQueue>();
        let snapshot_texture = world.resource::<RenderSnapshotTexture>();
        let images = world.resource::<RenderAssets<Image>>();
        // let draw_functions = world.resource::<DrawFunctions<SnapshotItem>>(); // Custom draw functions / 自定义绘制函数

        let Some(target_image) = images.get(&snapshot_texture.0) else {
            // Target texture not ready / 目标纹理未准备好
            return Ok(());
        };

        if snapshot_requests.requests.is_empty() {
            return Ok(()); // Nothing to snapshot / 无需快照
        }

        // --- This is the Hard Part ---
        // --- 这是困难的部分 ---
        // We need to issue separate render passes, each targeting a different
        // layer of the snapshot texture array. Bevy's default render graph
        // structure isn't designed for easily iterating render passes like this
        // within a single node run.
        // 我们需要发出单独的渲染通道，每个通道针对快照纹理数组的不同层。
        // Bevy 的默认渲染图结构不适合在单个节点运行中轻松迭代这样的渲染通道。

        // **Conceptual Loop:** / **概念循环:**
        for request in &snapshot_requests.requests {
            // 1. Create Texture View for the specific layer / 为特定层创建纹理视图
            let layer_view = target_image.texture.create_view(&TextureViewDescriptor {
                label: Some("snapshot_layer_view"),
                dimension: Some(TextureViewDimension::D2),
                base_array_layer: request.snapshot_layer_index,
                array_layer_count: Some(1),
                ..target_image.texture_view_descriptor.clone() // Inherit format etc. / 继承格式等
            });

            // 2. Define Render Pass Descriptor targeting this layer / 定义针对此层的 RenderPassDescriptor
            let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("snapshot_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &layer_view, // Target the specific layer / 针对特定层
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::NONE.into()), // Clear before drawing / 绘制前清除
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None, // No depth needed? Or use a temporary depth texture? / 不需要深度？或使用临时深度纹理？
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // 3. Set a Viewport/Scissor matching the chunk? (Usually handled by camera projection)
            //    设置匹配区块的视口/裁剪？(通常由相机投影处理)

            // 4. **Filter and Draw:** This requires the queued `SnapshotItem`s for *this specific chunk*.
            //    **过滤和绘制:** 这需要为 *这个特定区块* 排队的 `SnapshotItem`。
            //    The queuing system needs to associate items with chunks.
            //    排队系统需要将项与区块关联。
            //    render_pass.draw_render_phase::<SnapshotItem>(snapshot_phase_for_this_chunk, draw_functions);

            // **Simplified Placeholder:** Draw *something* to indicate it worked.
            // **简化占位符:** 绘制 *一些东西* 以表明它工作了。
            // (This would require a pipeline and bind groups set here)
            // (这将需要在此处设置管线和绑定组)
            // render_pass.set_pipeline(...);
            // render_pass.set_bind_group(...);
            // render_pass.draw(0..3, 0..1); // Draw a triangle / 绘制一个三角形

            // info!("Rendered snapshot for layer {}", request.snapshot_layer_index);
        }

        Ok(())
    }
}
