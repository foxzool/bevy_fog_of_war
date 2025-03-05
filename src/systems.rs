use crate::chunk::{Chunk, ChunkManager, ChunkNeedsGeneration};
use bevy::prelude::*;
use bevy::render::primitives::{Frustum, Aabb};
use bevy::math::Affine3A;

/// 在相机视野椎体内检查和生成区块的系统
/// System for checking and generating chunks within the camera frustum
pub fn check_and_generate_chunks_in_view(
    mut commands: Commands,
    chunk_manager: Res<ChunkManager>,
    camera_query: Query<(&Transform, &Frustum), With<Camera>>,
    existing_chunks: Query<(Entity, &Chunk, &Transform)>,
) {
    // 获取相机位置和视锥体
    // Get camera position and frustum
    let (camera_transform, frustum) = match camera_query.get_single() {
        Ok(data) => data,
        Err(_) => return, // 没有找到相机或找到多个相机
    };

    // 将相机位置转换为2D世界坐标
    // Convert camera position to 2D world coordinates
    let camera_position = Vec2::new(
        camera_transform.translation.x,
        camera_transform.translation.y,
    );

    // 获取基于距离的区块索引（作为初步筛选）
    // Get chunk indices based on distance (as initial filtering)
    let potential_chunks = chunk_manager.get_chunks_in_camera_view(camera_position);

    // 创建一个HashSet来存储现有的区块索引，以便快速查找
    // Create a HashSet to store existing chunk indices for quick lookup
    let mut existing_chunk_indices = std::collections::HashSet::new();
    let mut chunks_to_remove = Vec::new();

    // 滑后区域参数，用于减少闪烁
    // Hysteresis parameters to reduce flickering
    const INNER_BUFFER_FACTOR: f32 = 0.0;  // 内部缓冲区因子（用于添加区块）
    const OUTER_BUFFER_FACTOR: f32 = 0.5;  // 外部缓冲区因子（用于移除区块）
    
    // 收集现有区块的索引并标记视野外的区块以便移除
    // Collect indices of existing chunks and mark chunks outside the view for removal
    for (entity, chunk, transform) in existing_chunks.iter() {
        existing_chunk_indices.insert(chunk.index);

        // 创建区块的AABB包围盒用于视锥体检测
        // Create AABB bounding box for the chunk for frustum culling
        let chunk_min = Vec3::new(
            transform.translation.x,
            transform.translation.y,
            0.0,
        );
        let chunk_max = Vec3::new(
            transform.translation.x + chunk.size,
            transform.translation.y + chunk.size,
            0.0,
        );
        
        // 如果区块不在基础视野范围内，标记为移除
        // If the chunk is not in the basic view range, mark it for removal
        let in_distance_range = potential_chunks.contains(&chunk.index);
        
        // 创建一个更大的外部边界，用于决定何时移除区块
        // Create a larger outer boundary to decide when to remove chunks
        let outer_buffer = chunk.size * OUTER_BUFFER_FACTOR;
        let outer_min = Vec3::new(
            chunk_min.x - outer_buffer,
            chunk_min.y - outer_buffer,
            chunk_min.z
        );
        let outer_max = Vec3::new(
            chunk_max.x + outer_buffer,
            chunk_max.y + outer_buffer,
            chunk_max.z
        );
        
        // 检查区块是否在外部边界内
        // Check if the chunk is within the outer boundary
        let in_outer_boundary = is_aabb_in_frustum(outer_min, outer_max, frustum);
        
        // 只有当区块完全超出外部边界或不在距离范围内时才移除
        // Only remove the chunk when it is completely outside the outer boundary or not in distance range
        if !in_distance_range || !in_outer_boundary {
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
    for chunk_index in potential_chunks {
        if !existing_chunk_indices.contains(&chunk_index) {
            // 计算区块的世界坐标
            // Calculate world coordinates for the chunk
            let chunk_pos_x = chunk_index.x as f32 * chunk_manager.chunk_size;
            let chunk_pos_y = chunk_index.y as f32 * chunk_manager.chunk_size;
            
            // 创建区块的AABB包围盒用于视锥体检测
            // Create AABB bounding box for the chunk for frustum culling
            let chunk_min = Vec3::new(chunk_pos_x, chunk_pos_y, 0.0);
            let chunk_max = Vec3::new(
                chunk_pos_x + chunk_manager.chunk_size,
                chunk_pos_y + chunk_manager.chunk_size,
                0.0,
            );
            
            // 创建一个较小的内部边界，用于决定何时添加区块
            // Create a smaller inner boundary to decide when to add chunks
            let inner_buffer = chunk_manager.chunk_size * INNER_BUFFER_FACTOR;
            let inner_min = Vec3::new(
                chunk_min.x - inner_buffer,
                chunk_min.y - inner_buffer,
                chunk_min.z
            );
            let inner_max = Vec3::new(
                chunk_max.x + inner_buffer,
                chunk_max.y + inner_buffer,
                chunk_max.z
            );
            
            // 检查区块是否在内部边界内
            // Check if the chunk is within the inner boundary
            let in_inner_boundary = is_aabb_in_frustum(inner_min, inner_max, frustum);
            
            // 只有当区块在内部边界内时才创建它
            // Only create the chunk when it is within the inner boundary
            if in_inner_boundary {
                commands.spawn((
                    Chunk::new(chunk_index, chunk_manager.chunk_size),
                    ChunkNeedsGeneration,
                    Transform::from_translation(Vec3::new(
                        chunk_pos_x,
                        chunk_pos_y,
                        0.0,
                    )),
                    Visibility::default(),
                ));
            }
        }
    }
}

/// 检查AABB包围盒是否在视锥体内
/// Check if AABB bounding box is within the frustum
fn is_aabb_in_frustum(min: Vec3, max: Vec3, frustum: &Frustum) -> bool {
    // 创建AABB包围盒
    // Create an AABB bounding box
    let aabb = Aabb {
        center: ((min + max) * 0.5).into(),
        half_extents: ((max - min) * 0.5).into(),
    };
    
    // 使用Frustum的intersects_obb方法检查AABB是否与视锥体相交
    // Use Frustum's intersects_obb method to check if AABB intersects with frustum
    let world_from_local = Affine3A::from_translation(Vec3::ZERO);
    frustum.intersects_obb(&aabb, &world_from_local, true, true)
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
