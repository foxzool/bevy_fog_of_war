use crate::chunk::{Chunk, ChunkManager, ChunkNeedsGeneration};
use bevy::prelude::*;

/// 在相机视野椎体内检查和生成区块的系统
/// System for checking and generating chunks within the camera frustum
pub fn check_and_generate_chunks_in_view(
    mut commands: Commands,
    chunk_manager: Res<ChunkManager>,
    camera_query: Query<&Transform, With<Camera>>,
    existing_chunks: Query<(Entity, &Chunk)>,
) {
    // 获取相机位置
    // Get camera position
    let camera_transform = match camera_query.get_single() {
        Ok(transform) => transform,
        Err(_) => return, // 没有找到相机或找到多个相机
    };

    // 将相机位置转换为2D世界坐标
    // Convert camera position to 2D world coordinates
    let camera_position = Vec2::new(
        camera_transform.translation.x,
        camera_transform.translation.y,
    );

    // 获取相机视野内的所有区块索引
    // Get all chunk indices within the camera's view
    let chunks_in_view = chunk_manager.get_chunks_in_camera_view(camera_position);

    // 创建一个HashSet来存储现有的区块索引，以便快速查找
    // Create a HashSet to store existing chunk indices for quick lookup
    let mut existing_chunk_indices = std::collections::HashSet::new();
    let mut chunks_to_remove = Vec::new();

    // 收集现有区块的索引并标记视野外的区块以便移除
    // Collect indices of existing chunks and mark chunks outside the view for removal
    for (entity, chunk) in existing_chunks.iter() {
        existing_chunk_indices.insert(chunk.index);

        // 如果区块不在视野内，标记为移除
        // If the chunk is not in view, mark it for removal
        if !chunks_in_view.contains(&chunk.index) {
            chunks_to_remove.push(entity);
        }
    }

    // 移除视野外的区块
    // Remove chunks outside the view
    for entity in chunks_to_remove {
        commands.entity(entity).despawn();
    }

    // 为视野内但尚未创建的区块生成新的实体
    // Generate new entities for chunks in view but not yet created
    for chunk_index in chunks_in_view {
        if !existing_chunk_indices.contains(&chunk_index) {
            // 创建新的区块实体
            // Create a new chunk entity
            commands.spawn((
                Chunk::new(chunk_index, chunk_manager.chunk_size),
                ChunkNeedsGeneration, // 标记需要生成
                Transform::from_translation(Vec3::new(
                    chunk_index.x as f32 * chunk_manager.chunk_size,
                    chunk_index.y as f32 * chunk_manager.chunk_size,
                    0.0,
                )),
                Visibility::default(),
            ));
        }
    }
}

/// 处理需要生成的区块
/// Process chunks that need to be generated
pub fn process_chunk_generation(
    mut commands: Commands,
    chunks_query: Query<(Entity, &Chunk), With<ChunkNeedsGeneration>>,
    // 这里可以添加其他需要的资源，如纹理、材质等
) {
    for (entity, chunk) in chunks_query.iter() {
        // 在这里实现区块的实际生成逻辑
        // Implement actual chunk generation logic here

        // 例如，可以添加一个可视化组件来表示区块
        // For example, you can add a visualization component to represent the chunk
        commands
            .entity(entity)
            .insert(Sprite {
                color: Color::srgba(0.5, 0.5, 0.5, 0.2),
                custom_size: Some(Vec2::new(chunk.size, chunk.size)),
                ..default()
            })
            .insert(Transform::from_translation(Vec3::new(
                chunk.get_center_world_pos().x,
                chunk.get_center_world_pos().y,
                0.0,
            )))
            .remove::<ChunkNeedsGeneration>(); // 移除生成标记
    }
}

/// 调试系统：显示区块边界
/// Debug system: display chunk boundaries
pub fn debug_draw_chunk_boundaries(
    mut gizmos: Gizmos,
    chunks_query: Query<&Chunk>,
    chunk_manager: Res<ChunkManager>,
) {
    for chunk in chunks_query.iter() {
        let world_pos = chunk.get_world_pos();
        let size = chunk_manager.chunk_size;

        // 绘制区块边界
        // Draw chunk boundaries
        gizmos.rect_2d(
            Vec2::new(world_pos.x + size / 2.0, world_pos.y + size / 2.0),
            Vec2::new(size, size),
            Color::WHITE,
        );
    }
}
