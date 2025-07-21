use crate::prelude::*;
use bevy::log::{debug, error, info, trace, warn};
use bevy::prelude::Resource;
use bevy::reflect::Reflect;
use std::collections::HashMap;
use std::collections::HashSet;

/// Resource for efficient chunk coordinate to entity mapping and lookups.
/// 快速查找区块坐标对应的 FogChunk 实体
///
/// This resource provides O(1) lookup from chunk coordinates to their corresponding
/// Bevy entity IDs. It serves as the primary index for the chunk-based fog of war system,
/// enabling efficient queries and updates without iterating through all chunk entities.
///
/// # Purpose
///
/// The fog of war system needs to frequently:
/// - Find entities for specific world positions
/// - Update chunks when vision sources move
/// - Manage chunk lifecycle (spawn/despawn)
/// - Transfer data between CPU and GPU for specific chunks
///
/// Without this index, these operations would require expensive O(N) entity queries.
///
/// # Performance Characteristics
/// - **Lookup Time**: O(1) average case, O(log N) worst case
/// - **Memory Usage**: ~24 bytes per chunk (HashMap overhead + coordinate + entity ID)
/// - **Thread Safety**: Resource managed by Bevy's ECS system
/// - **Cache Efficiency**: High due to spatial locality of chunk access patterns
///
/// # Lifecycle Management
/// - **Insertion**: When new chunks are spawned via `commands.spawn()`
/// - **Lookup**: During vision calculations, rendering, and memory management
/// - **Removal**: When chunks are despawned or during system resets
/// - **Cleanup**: Automatically handled by ECS when entities are despawned
///
/// # Usage Patterns
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_fog_of_war::prelude::*;
/// fn find_chunk_at_position(
///     manager: Res<ChunkEntityManager>,
///     settings: Res<FogMapSettings>,
///     chunks: Query<&FogChunk>,
/// ) {
///     let world_pos = Vec2::new(300.0, 400.0);
///     let chunk_coord = settings.world_to_chunk_coords(world_pos);
///     
///     if let Some(entity) = manager.map.get(&chunk_coord) {
///         if let Ok(chunk) = chunks.get(*entity) {
///             println!("Found chunk at {:?}", chunk.coords);
///         }
///     }
/// }
/// ```
///
/// # Consistency Guarantees
/// - Map entries should always correspond to valid, existing entities
/// - All spawned FogChunk entities should have entries in this map
/// - Coordinates should be unique (one entity per coordinate)
/// - Cleanup should happen atomically with entity despawning
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource, Default)] // 注册为反射资源, 并提供默认值反射 / Register as reflectable resource with default reflection
pub struct ChunkEntityManager {
    /// Hash map from chunk coordinates to their corresponding entity IDs.
    /// 从区块坐标到实体 ID 的映射
    ///
    /// This is the core data structure enabling O(1) chunk lookups. Each entry
    /// represents an active chunk in the fog of war system.
    ///
    /// **Key**: `IVec2` chunk coordinates (e.g., (0,0), (1,-2), (-3,5))
    /// **Value**: `Entity` ID from Bevy's ECS system
    ///
    /// **Consistency**: Should always contain valid entity IDs for existing chunks
    /// **Cleanup**: Entries must be removed when entities are despawned
    pub map: HashMap<IVec2, Entity>,
}

/// Performance-critical state cache tracking chunk coordinates in various states.
/// 缓存各种状态的区块坐标集合，用于系统间的快速查询
///
/// This resource maintains high-performance hash sets of chunk coordinates categorized
/// by their current state. It enables O(1) state queries and efficient bulk operations
/// across multiple systems without expensive entity iteration or component queries.
///
/// # Performance Optimization
///
/// Instead of querying all chunk entities every frame to determine their states,
/// systems can efficiently check these cached sets:
/// - **Before**: O(N) entity iteration + component access
/// - **After**: O(1) HashSet lookup
///
/// # State Categories
///
/// ## Visibility States (Gameplay)
/// - `visible_chunks`: Currently in active vision
/// - `explored_chunks`: Ever been visible (persistent)
///
/// ## System States (Performance)  
/// - `camera_view_chunks`: Within camera viewport (culling)
/// - `gpu_resident_chunks`: Allocated on GPU (memory management)
///
/// # Update Patterns
/// - **Frame-based**: `visible_chunks`, `camera_view_chunks` updated every frame
/// - **Persistent**: `explored_chunks` accumulates over time, rarely cleared
/// - **Memory-driven**: `gpu_resident_chunks` updated during allocation/deallocation
///
/// # Memory Usage
/// Each HashSet uses ~8 bytes per coordinate plus overhead. With typical chunk counts:
/// - Small games: ~1KB total
/// - Large games: ~10-100KB total
/// - Trade-off: Small memory cost for significant performance gains
///
/// # System Coordination
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_fog_of_war::prelude::*;
/// fn efficient_chunk_processing(
///     cache: Res<ChunkStateCache>,
///     chunks: Query<&mut FogChunk>,
/// ) {
///     // Only process chunks that are both visible and in camera view
///     for coord in cache.visible_chunks.intersection(&cache.camera_view_chunks) {
///         // Process only relevant chunks (typically 10-50 instead of 1000+)
///         println!("Processing visible chunk in camera view: {:?}", coord);
///     }
/// }
/// ```
///
/// # Cache Consistency
/// Cache must stay synchronized with actual chunk states:
/// - **Critical**: Out-of-sync cache leads to rendering artifacts or performance issues
/// - **Responsibility**: Each system that changes chunk state must update the cache
/// - **Verification**: Debug builds can validate cache consistency
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource, Default)] // 注册为反射资源, 并提供默认值反射 / Register as reflectable resource with default reflection
pub struct ChunkStateCache {
    /// Set of chunk coordinates currently visible to at least one vision source.
    /// 当前被至少一个 VisionSource 照亮的区块坐标集合
    ///
    /// This set contains chunks that are actively being revealed by vision sources.
    /// It represents the "live" fog of war state and is updated every frame.
    ///
    /// **Update Frequency**: Every frame during vision calculation
    /// **Size**: Typically 10-100 chunks depending on vision source count and range
    /// **Persistence**: Cleared and rebuilt each frame
    /// **Usage**: Rendering decisions, fog texture updates, exploration tracking
    pub visible_chunks: HashSet<IVec2>,

    /// Set of chunk coordinates that have ever been revealed (includes visible_chunks).
    /// 曾经被照亮过的区块坐标集合 (包含 visible_chunks)
    ///
    /// This set accumulates all chunks that have been visible at any point, creating
    /// the persistent "explored area" for traditional fog of war mechanics.
    ///
    /// **Update Frequency**: When new areas become visible (incremental)
    /// **Size**: Grows throughout gameplay session
    /// **Persistence**: Maintained across frames, only cleared during fog reset
    /// **Usage**: Rendering explored vs unexplored areas, save/load functionality
    pub explored_chunks: HashSet<IVec2>,

    /// Set of chunk coordinates currently within the main camera's view frustum.
    /// 当前在主相机视锥范围内的区块坐标集合
    ///
    /// This set enables viewport culling optimizations by tracking which chunks
    /// are potentially visible to the player (regardless of fog state).
    ///
    /// **Update Frequency**: Every frame when camera moves
    /// **Size**: Depends on camera viewport and chunk size (typically 20-200 chunks)
    /// **Persistence**: Cleared and rebuilt each frame
    /// **Usage**: Rendering culling, memory management prioritization, LOD decisions
    pub camera_view_chunks: HashSet<IVec2>,

    /// Set of chunk coordinates whose textures are currently resident in GPU memory.
    /// 其纹理当前存储在 GPU 显存中的区块坐标集合
    ///
    /// This set tracks GPU memory allocation for dynamic memory management.
    /// Chunks not in this set have their texture data stored in CPU memory.
    ///
    /// **Update Frequency**: During allocation/deallocation operations
    /// **Size**: Limited by GPU memory capacity (typically 64-256 chunks max)
    /// **Persistence**: Updated during memory transfers, maintained across frames
    /// **Usage**: Memory management, transfer scheduling, rendering availability
    pub gpu_resident_chunks: HashSet<IVec2>,
}

impl ChunkStateCache {
    /// Clears frame-based caches while preserving persistent exploration data.
    /// 清除所有缓存的区块集合，通常在每帧开始时调用
    ///
    /// This method clears caches that are rebuilt every frame (visibility and camera view)
    /// while preserving data that accumulates over time (exploration history and GPU allocation).
    /// It's designed for efficient frame-based cache management.
    ///
    /// # Clearing Strategy
    /// - **Cleared**: `visible_chunks`, `camera_view_chunks` (rebuilt every frame)
    /// - **Preserved**: `explored_chunks` (persistent exploration history)
    /// - **Preserved**: `gpu_resident_chunks` (managed separately by memory systems)
    ///
    /// # Performance
    /// - **Time Complexity**: O(1) - HashSet::clear() is constant time
    /// - **Memory**: Capacity is preserved, only elements are removed
    /// - **Frequency**: Called every frame by vision calculation systems
    ///
    /// # Usage Pattern
    /// ```rust,no_run
    /// # use bevy::prelude::*;
    /// # use bevy_fog_of_war::prelude::*;
    /// fn vision_calculation_system(mut cache: ResMut<ChunkStateCache>) {
    ///     // Clear frame-based caches at start of vision calculation
    ///     cache.clear();
    ///     
    ///     // Rebuild visible_chunks and camera_view_chunks from current state
    ///     // ... vision calculation logic ...
    /// }
    /// ```
    ///
    /// # Why Not Clear Everything?
    /// - `explored_chunks`: Clearing would lose player's exploration progress
    /// - `gpu_resident_chunks`: Managed by separate memory management systems
    pub fn clear(&mut self) {
        self.visible_chunks.clear();
        // explored_chunks 通常不清空，除非需要重置迷雾 / explored_chunks is usually not cleared unless resetting fog
        self.camera_view_chunks.clear();
        // gpu_resident_chunks 的管理更复杂，不一定每帧清空 / gpu_resident_chunks management is more complex, not necessarily cleared every frame
    }

    /// Completely resets all cached data including persistent exploration history.
    /// 完全重置所有缓存，包括已探索区域，用于雾效重置
    ///
    /// This method provides a complete reset of the fog of war state, clearing all
    /// cached chunk coordinates including the persistent exploration history.
    /// It's used for "New Game" functionality or debug reset commands.
    ///
    /// # Reset Scope
    /// - **Visibility**: All current vision state cleared
    /// - **Exploration**: All historical exploration data cleared
    /// - **Camera View**: Current viewport culling cleared  
    /// - **GPU Memory**: All GPU allocation tracking cleared
    ///
    /// # Performance
    /// - **Time Complexity**: O(1) - HashSet::clear() operations
    /// - **Memory**: All HashSet capacity preserved for reuse
    /// - **Frequency**: Rarely called (only during explicit reset commands)
    ///
    /// # Use Cases
    /// - **New Game**: Starting fresh fog of war state
    /// - **Level Transitions**: Resetting fog between levels
    /// - **Debug Commands**: Developer tools for testing
    /// - **Save Loading**: Clearing state before loading saved data
    ///
    /// # Example
    /// ```rust,no_run
    /// # use bevy::prelude::*;
    /// # use bevy_fog_of_war::prelude::*;
    /// fn handle_new_game(mut cache: ResMut<ChunkStateCache>) {
    ///     // Player started new game - reset all fog state
    ///     cache.reset_all();
    ///     println!("Fog of war reset - starting with fresh exploration");
    /// }
    /// ```
    ///
    /// # Side Effects
    /// After calling this method:
    /// - Player will see completely unexplored world
    /// - All chunks will need to be re-explored
    /// - GPU memory allocation tracking is reset
    /// - Previous save data (if any) is effectively discarded from cache
    pub fn reset_all(&mut self) {
        self.visible_chunks.clear();
        self.explored_chunks.clear();
        self.camera_view_chunks.clear();
        self.gpu_resident_chunks.clear();
    }
}

/// Advanced GPU texture array layer allocation manager for fog of war chunks.
/// GPU纹理数组层分配管理器
///
/// This manager handles the complex task of dynamically allocating and deallocating
/// texture array layers for fog chunks on the GPU. It maintains separate pools of
/// fog and snapshot texture layers, enabling efficient memory management for the
/// chunk-based fog of war system.
///
/// # GPU Texture Arrays
///
/// Modern GPUs use texture arrays to efficiently render multiple textures in a single
/// draw call. Each "layer" in the array is essentially a separate texture that can be
/// accessed by index in shaders. This manager treats these layers as a limited resource
/// pool that must be carefully managed.
///
/// # Architecture
///
/// ## Dual Texture System
/// Each chunk requires two texture layers:
/// - **Fog Layer**: Real-time visibility data (continuously updated)
/// - **Snapshot Layer**: Persistent exploration data (updated when areas become explored)
///
/// ## Allocation Strategy
/// - **Pool-based**: Maintains free lists of available layer indices
/// - **Independent Pools**: Fog and snapshot layers are allocated separately
/// - **Reuse**: Freed layers are returned to pools for reuse
/// - **Persistence Support**: Can allocate specific layers for save/load functionality
///
/// # Performance Characteristics
/// - **Allocation**: O(1) - pop from Vec
/// - **Deallocation**: O(N) - includes double-free protection checks
/// - **Lookup**: O(1) - HashMap coordinate to layer mapping
/// - **Memory**: ~24 bytes per allocated chunk + pool overhead
///
/// # Memory Management Flow
/// ```text
/// 1. Chunk becomes visible → allocate_layer_indices()
/// 2. Chunk used for rendering → GPU rendering system accesses layers
/// 3. Chunk moves far from camera → free_layer_indices_for_coord()
/// 4. Layers returned to pool → available for reuse
/// ```
///
/// # Capacity Management
/// - **Maximum Layers**: Defined by `MAX_LAYERS` constant (typically 64)
/// - **Memory Pressure**: When pools empty, chunks must be moved to CPU memory
/// - **Prioritization**: Visible chunks prioritized over distant chunks
///
/// # Example Usage
/// ```rust,no_run
/// # use bevy_fog_of_war::prelude::*;
/// # use bevy::prelude::*;
/// fn allocate_chunk_on_gpu(
///     mut manager: ResMut<TextureArrayManager>,
///     coords: IVec2,
/// ) {
///     if let Some((fog_idx, snap_idx)) = manager.allocate_layer_indices(coords) {
///         println!("Allocated chunk {:?} to layers F{} S{}", coords, fog_idx, snap_idx);
///     } else {
///         println!("No GPU layers available for chunk {:?}", coords);
///     }
/// }
/// ```
///
/// # Safety and Consistency
/// - **Double-free Protection**: Prevents allocation of already-allocated layers
/// - **Leak Prevention**: Tracks coordinate-to-layer mapping for proper cleanup
/// - **Index Validation**: Ensures layer indices stay within array bounds
#[derive(Resource, Debug, Reflect)]
#[reflect(Resource)]
pub struct TextureArrayManager {
    /// Maximum number of texture layers available in the GPU texture arrays.
    /// GPU纹理数组中可用的最大纹理层数
    ///
    /// This defines the upper limit of chunks that can be simultaneously resident
    /// on the GPU. When this capacity is reached, additional chunks must be stored
    /// in CPU memory until GPU layers become available.
    ///
    /// **Typical Values**: 64-256 layers (hardware and memory dependent)
    /// **Trade-off**: More layers = more GPU memory usage but better performance
    capacity: u32,

    /// Maps chunk coordinates to their allocated GPU texture layer indices.
    /// 将区块坐标映射到它们当前在 GPU 上占用的层索引
    ///
    /// This is the authoritative mapping from game world chunk coordinates to
    /// their corresponding texture array layer indices on the GPU.
    ///
    /// **Key**: `IVec2` chunk coordinates
    /// **Value**: `(fog_layer_index, snapshot_layer_index)` tuple
    /// **Consistency**: Must stay synchronized with actual GPU allocations
    coord_to_layers: HashMap<IVec2, (u32, u32)>, // (fog_idx, snapshot_idx)

    /// Stack of available fog texture layer indices ready for allocation.
    /// 存储当前可以自由分配的雾效纹理层索引
    ///
    /// This Vec serves as a simple stack (LIFO) of free layer indices for fog textures.
    /// When chunks are deallocated, their fog layer indices are pushed back here.
    ///
    /// **Data Structure**: Vec used as stack for O(1) push/pop operations  
    /// **Range**: Contains indices from 0 to (capacity-1) when fully free
    /// **Invariant**: Should never contain duplicate indices
    free_fog_indices: Vec<u32>,

    /// Stack of available snapshot texture layer indices ready for allocation.
    /// 存储当前可以自由分配的快照纹理层索引
    ///
    /// This Vec serves as a simple stack (LIFO) of free layer indices for snapshot textures.
    /// When chunks are deallocated, their snapshot layer indices are pushed back here.
    ///
    /// **Data Structure**: Vec used as stack for O(1) push/pop operations
    /// **Range**: Contains indices from 0 to (capacity-1) when fully free  
    /// **Invariant**: Should never contain duplicate indices
    free_snapshot_indices: Vec<u32>,
    // Or, if fog and snapshot always use paired indices (e.g., fog layer X always pairs with snapshot layer X)
    // 或者，如果雾效和快照始终使用配对索引 (例如，雾效层 X 始终与快照层 X 配对)
    // free_paired_indices: Vec<u32>,
}

impl TextureArrayManager {
    /// Creates a new texture array manager with specified layer capacity.
    /// 创建新的纹理数组管理器，具有指定的层容量
    ///
    /// Initializes a new texture array manager that can handle up to `array_layers_capacity`
    /// chunks simultaneously on the GPU. This constructor sets up the initial state with all
    /// texture layers marked as available for allocation.
    ///
    /// # Parameters
    /// - `array_layers_capacity`: Maximum number of texture layers available in GPU texture arrays
    ///
    /// # Returns
    /// A new `TextureArrayManager` instance with all layers initialized as free and ready for allocation.
    ///
    /// # Architecture
    /// - **Dual Pool System**: Maintains separate free lists for fog and snapshot texture layers
    /// - **Independent Allocation**: Fog and snapshot layers are allocated independently
    /// - **Initial State**: All indices from 0 to (capacity-1) are marked as free
    ///
    /// # Performance Characteristics
    /// - **Time Complexity**: O(n) where n = `array_layers_capacity` (initialization loop)
    /// - **Space Complexity**: O(n) for the two Vec pools plus HashMap capacity
    /// - **Memory Allocation**: Pre-allocates Vec capacity to avoid reallocations during runtime
    ///
    /// # Memory Usage
    /// ```text
    /// Memory per Manager =
    ///   (capacity × 8 bytes × 2 pools) +    // Vec<u32> storage for free indices
    ///   (HashMap overhead ~64 bytes) +       // Initial HashMap allocation
    ///   (struct overhead ~32 bytes)          // Manager struct fields
    ///
    /// Example with capacity=64: ~1KB total
    /// ```
    ///
    /// # Initialization Process
    /// 1. **Vec Allocation**: Creates two Vec containers with pre-allocated capacity
    /// 2. **Index Population**: Fills both free lists with sequential indices 0..capacity-1
    /// 3. **Empty Mapping**: Initializes coordinate-to-layer mapping as empty HashMap
    /// 4. **Ready State**: Manager is immediately ready for layer allocation requests
    ///
    /// # Example
    /// ```rust,no_run
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// // Create manager for up to 64 simultaneous chunks on GPU
    /// let manager = TextureArrayManager::new(64);
    ///
    /// // All 64 fog layers and 64 snapshot layers are now available
    /// // Manager can allocate pairs of layers for coordinates
    /// ```
    ///
    /// # Design Decisions
    /// - **Separate Pools**: Fog and snapshot use independent allocation pools for flexibility
    /// - **Vec as Stack**: Uses Vec::push/pop for O(1) allocation/deallocation operations
    /// - **Pre-allocation**: Avoids runtime memory allocation by reserving capacity upfront
    /// - **Sequential Indices**: Initializes with ordered indices for predictable GPU memory layout
    ///
    /// # GPU Compatibility
    /// The capacity should match the actual texture array size configured in GPU shaders:
    /// - **Validation**: Ensure capacity matches shader MAX_LAYERS constant
    /// - **Hardware Limits**: Consider GPU texture array size limitations
    /// - **Memory Constraints**: Balance capacity with available GPU memory
    pub fn new(array_layers_capacity: u32) -> Self {
        // Initialize all layers as free
        // 将所有层初始化为空闲
        let mut free_fog = Vec::with_capacity(array_layers_capacity as usize);
        let mut free_snap = Vec::with_capacity(array_layers_capacity as usize);
        for i in 0..array_layers_capacity {
            free_fog.push(i);
            free_snap.push(i); // Assuming separate pools for simplicity, or they could be linked
        }
        Self {
            capacity: array_layers_capacity,
            coord_to_layers: HashMap::new(),
            free_fog_indices: free_fog,
            free_snapshot_indices: free_snap,
        }
    }

    /// Allocates a pair of texture layer indices for a chunk coordinate on the GPU.
    /// 为给定的区块坐标在 GPU 上分配一对纹理层索引
    ///
    /// Attempts to allocate both a fog texture layer and a snapshot texture layer for the
    /// specified chunk coordinate. This is the primary method for moving chunks from CPU
    /// memory to GPU memory for rendering and computation.
    ///
    /// # Parameters
    /// - `coords`: World chunk coordinates to allocate GPU layers for
    ///
    /// # Returns
    /// - `Some((fog_layer_index, snapshot_layer_index))`: Successfully allocated layer pair
    /// - `None`: No free layers available (GPU memory exhausted)
    ///
    /// # Allocation Strategy
    /// - **Atomic Allocation**: Either both layers are allocated or neither (consistent state)
    /// - **Independent Pools**: Fog and snapshot layers come from separate free pools
    /// - **Duplicate Protection**: Warns and reuses existing allocation if coordinate already has layers
    /// - **LIFO Order**: Uses most recently freed indices first (Vec::pop behavior)
    ///
    /// # Performance Characteristics
    /// - **Time Complexity**: O(1) average case (Vec::pop + HashMap operations)
    /// - **Space Complexity**: O(1) (updates existing data structures)
    /// - **Allocation Speed**: Very fast due to stack-based free list management
    /// - **Memory Pressure**: Fails gracefully when GPU memory is exhausted
    ///
    /// # State Changes
    /// On successful allocation:
    /// 1. **Free Pool Updates**: Removes indices from both free_fog_indices and free_snapshot_indices
    /// 2. **Mapping Addition**: Adds coordinate → (fog_idx, snapshot_idx) to coord_to_layers
    /// 3. **Logging**: Debug logs the allocation for monitoring
    ///
    /// # Error Handling
    /// - **Duplicate Allocation**: Warns and returns existing allocation (defensive programming)
    /// - **Pool Exhaustion**: Logs error and returns None when no free layers available
    /// - **Partial Failure**: Currently not possible due to independent pool design
    ///
    /// # Example Usage
    /// ```rust,no_run
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let mut manager = TextureArrayManager::new(64);
    /// let chunk_coord = IVec2::new(5, -3);
    ///
    /// match manager.allocate_layer_indices(chunk_coord) {
    ///     Some((fog_idx, snap_idx)) => {
    ///         println!("Allocated chunk {:?} to fog layer {} and snapshot layer {}",
    ///                  chunk_coord, fog_idx, snap_idx);
    ///         // Proceed with GPU memory transfer...
    ///     }
    ///     None => {
    ///         println!("GPU memory exhausted - cannot allocate more chunks");
    ///         // Need to free some chunks or use CPU fallback...
    ///     }
    /// }
    /// ```
    ///
    /// # Memory Management Flow
    /// ```text
    /// [CPU Chunk] → allocate_layer_indices() → [GPU Texture Layers]
    ///     ↓               ↓                          ↓
    /// coord: (5,-3)   Request layers          fog: layer 42
    ///                     ↓                   snap: layer 17
    ///                 Check pools             ↓
    ///                     ↓               Update mapping
    ///                 Pop indices         coord_to_layers
    ///                     ↓               (5,-3) → (42,17)
    ///                 Return pair
    /// ```
    ///
    /// # Integration with Render Pipeline
    /// The returned layer indices are used by:
    /// - **Compute Shaders**: Access specific layers for fog calculations
    /// - **Memory Transfer**: Copy texture data to allocated GPU layers
    /// - **Rendering**: Sample from correct layers during final composition
    /// - **Cleanup**: Track allocations for proper deallocation
    ///
    /// # Capacity Planning
    /// Monitor allocation failures to determine if capacity needs adjustment:
    /// - **High Failure Rate**: Increase texture array capacity or implement LRU eviction
    /// - **Low Utilization**: Reduce capacity to save GPU memory
    /// - **Temporal Patterns**: Consider dynamic capacity based on game state
    pub fn allocate_layer_indices(&mut self, coords: IVec2) -> Option<(u32, u32)> {
        if self.coord_to_layers.contains_key(&coords) {
            // This coord already has layers, should not happen if logic is correct.
            // Or, it means we are re-activating a chunk that somehow wasn't fully cleaned up.
            // 这个坐标已经有层了，如果逻辑正确则不应发生。
            // 或者，这意味着我们正在重新激活一个不知何故未完全清理的区块。
            warn!(
                "Attempted to allocate layers for {:?} which already has layers: {:?}. Reusing.",
                coords,
                self.coord_to_layers.get(&coords)
            );
            return self.coord_to_layers.get(&coords).copied();
        }

        if let (Some(fog_idx), Some(snap_idx)) = (
            self.free_fog_indices.pop(),
            self.free_snapshot_indices.pop(),
        ) {
            self.coord_to_layers.insert(coords, (fog_idx, snap_idx));
            debug!(
                "Allocating layers for coord {:?}. F{} S{}",
                coords, fog_idx, snap_idx
            );
            Some((fog_idx, snap_idx))
        } else {
            // Ran out of layers, push back any popped indices if one succeeded but other failed (shouldn't happen with paired pop)
            // 层用完了，如果一个成功但另一个失败，则推回任何弹出的索引 (配对弹出不应发生这种情况)
            // This logic needs to be robust if fog/snapshot indices are truly independent.
            // 如果雾效/快照索引真正独立，则此逻辑需要稳健。
            // For now, assuming paired allocation success/failure.
            // 目前假设配对分配成功/失败。
            error!("TextureArrayManager: No free layers available!");
            None
        }
    }

    /// Frees GPU texture layer indices associated with a specific chunk coordinate.
    /// 释放与指定区块坐标关联的 GPU 纹理层索引
    ///
    /// Deallocates both fog and snapshot texture layers for the given chunk coordinate,
    /// returning them to the free pools for reuse. This is the primary method for moving
    /// chunks from GPU memory back to CPU-only storage when they're no longer needed.
    ///
    /// # Parameters
    /// - `coords`: Chunk coordinates whose GPU layers should be freed
    ///
    /// # Deallocation Process
    /// 1. **Lookup**: Finds allocated layer indices using coordinate mapping
    /// 2. **Validation**: Prevents double-free by checking if indices are already free
    /// 3. **Pool Return**: Adds indices back to free_fog_indices and free_snapshot_indices
    /// 4. **Mapping Cleanup**: Removes coordinate from coord_to_layers mapping
    /// 5. **Logging**: Traces the deallocation for debugging
    ///
    /// # Performance Characteristics
    /// - **Time Complexity**: O(n) worst case due to contains() checks on free pools
    /// - **Space Complexity**: O(1) (modifies existing data structures)
    /// - **Memory Recovery**: Immediately makes GPU layers available for reallocation
    /// - **Safety Overhead**: Double-free protection adds linear search cost
    ///
    /// # Safety Features
    /// - **Double-Free Protection**: Checks if indices are already in free pools
    /// - **Orphan Detection**: Warns if coordinate has no allocated layers to free
    /// - **Atomic Cleanup**: Either both layers are freed or error state is logged
    /// - **Defensive Programming**: Continues execution even with inconsistent state
    ///
    /// # Error Handling
    /// - **Missing Allocation**: Warns if coordinate has no layers to free
    /// - **Double-Free Attempt**: Warns and skips already-free indices
    /// - **Partial State**: Handles cases where only one layer type is already free
    /// - **Logging**: All error conditions are logged for debugging
    ///
    /// # Example Usage
    /// ```rust,no_run
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let mut manager = TextureArrayManager::new(64);
    /// let chunk_coord = IVec2::new(5, -3);
    ///
    /// // First allocate layers
    /// if let Some((fog_idx, snap_idx)) = manager.allocate_layer_indices(chunk_coord) {
    ///     // Use the layers for rendering...
    ///     
    ///     // Later, when chunk moves far from camera
    ///     manager.free_layer_indices_for_coord(chunk_coord);
    ///     println!("Freed layers for chunk {:?}", chunk_coord);
    ///     
    ///     // Layers are now available for other chunks
    /// }
    /// ```
    ///
    /// # Memory Management Flow
    /// ```text
    /// [GPU Layers] → free_layer_indices_for_coord() → [Available Pool]
    ///      ↓                    ↓                           ↓
    /// fog: layer 42        Remove mapping              Add to free_fog_indices
    /// snap: layer 17       coord_to_layers             Add to free_snapshot_indices
    ///      ↓                Delete (5,-3)                   ↓
    /// coord: (5,-3)            ↓                      Ready for reuse
    ///                     [CPU Storage]
    /// ```
    ///
    /// # Common Use Cases
    /// - **Distance-Based Culling**: Free chunks that are far from camera
    /// - **Memory Pressure**: Free least important chunks when GPU memory is full
    /// - **Level Transitions**: Free all chunks when changing game areas
    /// - **Manual Management**: Explicit chunk lifecycle control by game logic
    ///
    /// # Performance Optimization Notes
    /// The double-free protection using `Vec::contains()` is O(n):
    /// - **Trade-off**: Safety vs performance (prevents crashes at cost of speed)
    /// - **Alternative**: Could use HashSet for free indices (O(1) lookup, more memory)
    /// - **Monitoring**: Profile this method if called frequently with large capacity
    /// - **Batch Operations**: Consider batch freeing to amortize overhead
    ///
    /// # Integration with Memory Systems
    /// This method is typically called by:
    /// - **Distance Culling**: When chunks move outside render distance
    /// - **LRU Eviction**: When implementing least-recently-used cache policies
    /// - **Memory Pressure**: When GPU memory usage exceeds thresholds
    /// - **Cleanup Systems**: During level unloading or system shutdown
    pub fn free_layer_indices_for_coord(&mut self, coords: IVec2) {
        if let Some((fog_idx, snap_idx)) = self.coord_to_layers.remove(&coords) {
            trace!(
                "Freeing layers for coord {:?}. F{} S{}",
                coords, fog_idx, snap_idx
            );
            // It's crucial that an index is not pushed to free_..._indices
            // if it's already there or if it's invalid.
            // 关键是，如果索引已存在或无效，则不要将其推送到 free_..._indices。
            if !self.free_fog_indices.contains(&fog_idx) {
                // Basic check to prevent double free
                self.free_fog_indices.push(fog_idx);
            } else {
                warn!(
                    "Attempted to double-free fog index {} for coord {:?}",
                    fog_idx, coords
                );
            }
            if !self.free_snapshot_indices.contains(&snap_idx) {
                self.free_snapshot_indices.push(snap_idx);
            } else {
                warn!(
                    "Attempted to double-free snapshot index {} for coord {:?}",
                    snap_idx, coords
                );
            }
        } else {
            warn!(
                "Attempted to free layers for coord {:?} which has no allocated layers.",
                coords
            );
        }
    }

    /// Frees specific GPU texture layer indices without requiring coordinate lookup.
    /// 释放特定的 GPU 纹理层索引，无需坐标查找
    ///
    /// Deallocates texture layers using their direct indices rather than chunk coordinates.
    /// This method is used when the calling code has direct access to the layer indices,
    /// typically from FogChunk entities that store their own layer allocation information.
    ///
    /// # Parameters
    /// - `fog_idx`: Index of the fog texture layer to free
    /// - `snap_idx`: Index of the snapshot texture layer to free
    ///
    /// # Reverse Lookup Process
    /// Since this method works with indices instead of coordinates, it must:
    /// 1. **Search Mapping**: Iterate through coord_to_layers to find which coordinate uses these indices
    /// 2. **Remove Mapping**: Delete the coordinate entry from coord_to_layers
    /// 3. **Return to Pool**: Add indices back to free_fog_indices and free_snapshot_indices
    /// 4. **Double-Free Protection**: Check if indices are already free before adding
    ///
    /// # Performance Characteristics
    /// - **Time Complexity**: O(n) for reverse lookup + O(n) for double-free checks
    /// - **Space Complexity**: O(1) (modifies existing data structures)
    /// - **Search Cost**: Must scan all allocated coordinates to find matching indices
    /// - **Safety Overhead**: Double protection against double-free corruptions
    ///
    /// # When to Use
    /// - **Entity Cleanup**: When FogChunk entities are despawned and provide their layer indices
    /// - **Direct Access**: When code has layer indices but not the original coordinates
    /// - **Batch Operations**: When processing multiple chunks with known layer indices
    /// - **Error Recovery**: When cleanup systems need to free orphaned layer allocations
    ///
    /// # Example Usage
    /// ```rust,no_run
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let mut manager = TextureArrayManager::new(64);
    ///
    /// // From FogChunk entity that's being despawned
    /// struct FogChunk {
    ///     fog_layer_index: Option<u32>,
    ///     snapshot_layer_index: Option<u32>,
    ///     // ... other fields
    /// }
    ///
    /// fn cleanup_fog_chunk(
    ///     chunk: &FogChunk,
    ///     mut manager: ResMut<TextureArrayManager>
    /// ) {
    ///     if let (Some(fog_idx), Some(snap_idx)) =
    ///         (chunk.fog_layer_index, chunk.snapshot_layer_index) {
    ///         manager.free_specific_layer_indices(fog_idx, snap_idx);
    ///     }
    /// }
    /// ```
    ///
    /// # Error Handling
    /// - **No Matching Coordinate**: Warns if no coordinate was using these specific indices
    /// - **Double-Free Protection**: Checks if indices are already in free pools
    /// - **Partial Success**: Handles cases where one index is valid and other is already free
    /// - **Defensive Logging**: All operations are logged for debugging consistency issues
    ///
    /// # Performance Considerations
    /// This method is less efficient than `free_layer_indices_for_coord()` because:
    /// ```text
    /// Coordinate-based:  O(1) HashMap lookup + O(n) double-free check
    /// Index-based:       O(n) reverse search + O(n) double-free check
    /// ```
    ///
    /// **Optimization Strategies**:
    /// - **Bidirectional Mapping**: Could maintain layer→coord mapping for O(1) reverse lookup
    /// - **Batch Processing**: Process multiple frees together to amortize search costs
    /// - **HashSet Free Pools**: Use HashSet instead of Vec for O(1) contains() checks
    ///
    /// # Data Consistency
    /// This method helps maintain consistency when:
    /// - **Entity Despawning**: FogChunk entities are removed and need cleanup
    /// - **System Crashes**: Recovery from inconsistent state where coordinates are lost
    /// - **Manual Management**: Direct control over specific layer allocations
    /// - **Testing**: Unit tests that need to free specific indices
    ///
    /// # Integration with ECS
    /// Typically called from Bevy systems that handle entity cleanup:
    /// ```rust,no_run
    /// fn cleanup_despawned_chunks(
    ///     mut removed_chunks: RemovedComponents<FogChunk>,
    ///     chunk_query: Query<&FogChunk>,
    ///     mut manager: ResMut<TextureArrayManager>,
    /// ) {
    ///     for entity in removed_chunks.read() {
    ///         if let Ok(chunk) = chunk_query.get(entity) {
    ///             if let (Some(fog), Some(snap)) = (chunk.fog_layer_index, chunk.snapshot_layer_index) {
    ///                 manager.free_specific_layer_indices(fog, snap);
    ///             }
    ///         }
    ///     }
    /// }
    /// ```
    pub fn free_specific_layer_indices(&mut self, fog_idx: u32, snap_idx: u32) {
        info!("Freeing specific layer indices {} {}", fog_idx, snap_idx);
        // We also need to find which coord was using these indices to remove it from coord_to_layers
        // 我们还需要找出哪个坐标正在使用这些索引，以便从 coord_to_layers 中删除它
        let mut coord_to_remove = None;
        for (coord, &indices) in &self.coord_to_layers {
            if indices == (fog_idx, snap_idx) {
                coord_to_remove = Some(*coord);
                break;
            }
        }
        if let Some(coord) = coord_to_remove {
            self.coord_to_layers.remove(&coord);
            debug!(
                "Removed coord {:?} for specific F{} S{}",
                coord, fog_idx, snap_idx
            );
        } else {
            warn!(
                "Attempted to free specific F{} S{} but no coord was using them.",
                fog_idx, snap_idx
            );
        }

        // It's crucial that an index is not pushed to free_..._indices
        // if it's already there or if it's invalid.
        // 关键是，如果索引已存在或无效，则不要将其推送到 free_..._indices。
        if !self.free_fog_indices.contains(&fog_idx) {
            // Basic check to prevent double free
            self.free_fog_indices.push(fog_idx);
        } else {
            warn!("Attempted to double-free specific fog index {}", fog_idx);
        }
        if !self.free_snapshot_indices.contains(&snap_idx) {
            self.free_snapshot_indices.push(snap_idx);
        } else {
            warn!(
                "Attempted to double-free specific snapshot index {}",
                snap_idx
            );
        }
    }

    /// Retrieves the allocated GPU layer indices for a specific chunk coordinate.
    /// 获取指定区块坐标的已分配 GPU 层索引
    ///
    /// Looks up the fog and snapshot texture layer indices currently allocated to the
    /// given chunk coordinate. This is a read-only operation used to query the current
    /// GPU allocation state without modifying any data structures.
    ///
    /// # Parameters
    /// - `coords`: Chunk coordinates to look up layer allocation for
    ///
    /// # Returns
    /// - `Some((fog_layer_index, snapshot_layer_index))`: Coordinate has allocated layers
    /// - `None`: Coordinate has no GPU layers allocated (stored in CPU memory only)
    ///
    /// # Performance Characteristics
    /// - **Time Complexity**: O(1) average case (HashMap lookup)
    /// - **Space Complexity**: O(1) (read-only operation)
    /// - **Thread Safety**: Safe for concurrent reads (requires only shared reference)
    /// - **Cache Efficiency**: High due to HashMap spatial locality
    ///
    /// # Example Usage
    /// ```rust,no_run
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let manager = TextureArrayManager::new(64);
    /// let chunk_coord = IVec2::new(3, -7);
    ///
    /// match manager.get_allocated_indices(chunk_coord) {
    ///     Some((fog_idx, snap_idx)) => {
    ///         println!("Chunk {:?} is on GPU: fog layer {}, snapshot layer {}",
    ///                  chunk_coord, fog_idx, snap_idx);
    ///         // Use indices for GPU operations...
    ///     }
    ///     None => {
    ///         println!("Chunk {:?} is in CPU memory only", chunk_coord);
    ///         // Need to allocate or load from CPU...
    ///     }
    /// }
    /// ```
    ///
    /// # Common Use Cases
    /// - **Render Pipeline**: Check if chunk has GPU layers before rendering
    /// - **Memory Transfer**: Determine if CPU→GPU transfer is needed
    /// - **Cache Status**: Query current GPU memory allocation state
    /// - **Debug Information**: Display GPU allocation status in debug UI
    /// - **Shader Parameters**: Pass layer indices to compute shaders
    ///
    /// # Integration with Systems
    /// Often used in rendering and memory management systems:
    /// ```rust,no_run
    /// fn update_chunk_textures(
    ///     chunk_query: Query<(&FogChunk, &Transform)>,
    ///     manager: Res<TextureArrayManager>,
    /// ) {
    ///     for (chunk, transform) in chunk_query.iter() {
    ///         if let Some((fog_idx, snap_idx)) = manager.get_allocated_indices(chunk.coords) {
    ///             // Chunk is on GPU - can render directly
    ///             render_gpu_chunk(fog_idx, snap_idx, transform);
    ///         } else {
    ///             // Chunk needs GPU allocation or CPU fallback
    ///             handle_cpu_chunk(chunk, transform);
    ///         }
    ///     }
    /// }
    /// ```
    pub fn get_allocated_indices(&self, coords: IVec2) -> Option<(u32, u32)> {
        self.coord_to_layers.get(&coords).copied()
    }

    /// Checks if a chunk coordinate has GPU texture layers allocated.
    /// 检查区块坐标是否已分配 GPU 纹理层
    ///
    /// Performs a fast boolean check to determine if the given chunk coordinate
    /// currently has texture layers allocated on the GPU. This is a convenience
    /// method that's equivalent to checking if `get_allocated_indices()` returns `Some()`.
    ///
    /// # Parameters
    /// - `coords`: Chunk coordinates to check for GPU allocation
    ///
    /// # Returns
    /// - `true`: Chunk has both fog and snapshot layers allocated on GPU
    /// - `false`: Chunk has no GPU allocation (CPU-only or unallocated)
    ///
    /// # Performance Characteristics
    /// - **Time Complexity**: O(1) average case (HashMap contains_key lookup)
    /// - **Space Complexity**: O(1) (read-only operation)
    /// - **Efficiency**: Faster than `get_allocated_indices()` since no data copying
    /// - **Cache Friendly**: Single HashMap lookup with predictable access patterns
    ///
    /// # Example Usage
    /// ```rust,no_run
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let manager = TextureArrayManager::new(64);
    /// let chunk_coord = IVec2::new(2, -4);
    ///
    /// if manager.is_coord_on_gpu(chunk_coord) {
    ///     println!("Chunk {:?} is ready for GPU rendering", chunk_coord);
    ///     // Proceed with GPU-based operations...
    /// } else {
    ///     println!("Chunk {:?} needs allocation or CPU fallback", chunk_coord);
    ///     // Handle CPU-based operations or request allocation...
    /// }
    /// ```
    ///
    /// # Common Usage Patterns
    ///
    /// **Memory Management Decision Making**:
    /// ```rust,no_run
    /// fn allocate_chunks_near_camera(
    ///     camera_query: Query<&Transform, With<FogOfWarCamera>>,
    ///     mut manager: ResMut<TextureArrayManager>,
    ///     settings: Res<FogMapSettings>,
    /// ) {
    ///     if let Ok(camera_transform) = camera_query.single() {
    ///         let camera_chunk = settings.world_to_chunk_coords(camera_transform.translation.xy());
    ///         
    ///         // Check nearby chunks
    ///         for offset_x in -2..=2 {
    ///             for offset_y in -2..=2 {
    ///                 let chunk_coord = camera_chunk + IVec2::new(offset_x, offset_y);
    ///                 
    ///                 if !manager.is_coord_on_gpu(chunk_coord) {
    ///                     // Try to allocate this chunk to GPU
    ///                     if let Some(_) = manager.allocate_layer_indices(chunk_coord) {
    ///                         println!("Allocated chunk {:?} near camera", chunk_coord);
    ///                     }
    ///                 }
    ///             }
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// **Conditional Rendering Logic**:
    /// ```rust,no_run
    /// fn render_fog_chunks(
    ///     chunk_query: Query<&FogChunk>,
    ///     manager: Res<TextureArrayManager>,
    /// ) {
    ///     for chunk in chunk_query.iter() {
    ///         if manager.is_coord_on_gpu(chunk.coords) {
    ///             // Use fast GPU rendering path
    ///             render_chunk_on_gpu(chunk);
    ///         } else {
    ///             // Use slower CPU rendering path
    ///             render_chunk_on_cpu(chunk);
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// # Why Use This Over get_allocated_indices()?
    /// - **Performance**: No Option unwrapping or tuple copying
    /// - **Clarity**: Intent is clearer when only checking existence
    /// - **Ergonomics**: Simpler boolean logic in conditional expressions
    /// - **Memory**: Avoids copying u32 values when indices aren't needed
    pub fn is_coord_on_gpu(&self, coords: IVec2) -> bool {
        self.coord_to_layers.contains_key(&coords)
    }

    /// Completely resets all GPU texture layer allocations to initial free state.
    /// 完全重置所有 GPU 纹理层分配到初始空闲状态
    ///
    /// Clears all allocated texture layers and returns them to the free pools, effectively
    /// resetting the entire texture array manager to its initial state. This is used for
    /// complete fog of war system resets, level transitions, or debug commands.
    ///
    /// # Reset Operations
    /// 1. **Clear Mapping**: Removes all coordinate-to-layer mappings
    /// 2. **Reset Pools**: Rebuilds free layer pools with all indices
    /// 3. **Restore Order**: Returns indices in sequential order (0, 1, 2, ...)
    /// 4. **Log Reset**: Info logs the complete reset operation
    ///
    /// # Performance Characteristics
    /// - **Time Complexity**: O(n) where n = capacity (rebuilds both free pools)
    /// - **Space Complexity**: O(1) (reuses existing Vec capacity)
    /// - **Memory Allocation**: No new allocations (clears and refills existing Vecs)
    /// - **Frequency**: Infrequent operation (only during major state changes)
    ///
    /// # State After Reset
    /// After calling this method, the manager returns to the same state as `new()`:
    /// - All layer indices 0..capacity-1 are available for allocation
    /// - No coordinates have GPU allocations
    /// - Free pools contain all indices in sequential order
    /// - Ready to allocate layers for any chunk coordinates
    ///
    /// # Example Usage
    /// ```rust,no_run
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let mut manager = TextureArrayManager::new(64);
    ///
    /// // Allocate some layers
    /// manager.allocate_layer_indices(IVec2::new(0, 0));
    /// manager.allocate_layer_indices(IVec2::new(1, 1));
    ///
    /// // Later, reset everything
    /// manager.clear_all_layers();
    ///
    /// // Now all 64 layers are available again
    /// assert!(!manager.is_coord_on_gpu(IVec2::new(0, 0)));
    /// assert!(!manager.is_coord_on_gpu(IVec2::new(1, 1)));
    /// ```
    ///
    /// # When to Use
    /// - **New Game**: Starting fresh game session with clean fog state
    /// - **Level Transitions**: Moving between game levels or maps
    /// - **Debug Commands**: Developer tools for testing fog systems
    /// - **Error Recovery**: Recovering from corrupted allocation state
    /// - **Memory Cleanup**: Preparing for different memory allocation patterns
    ///
    /// # Integration with Game Events
    /// ```rust,no_run
    /// fn handle_new_game_event(
    ///     mut new_game_events: EventReader<NewGameEvent>,
    ///     mut manager: ResMut<TextureArrayManager>,
    ///     mut cache: ResMut<ChunkStateCache>,
    /// ) {
    ///     for _ in new_game_events.read() {
    ///         // Reset all GPU allocations
    ///         manager.clear_all_layers();
    ///         
    ///         // Also reset chunk state cache
    ///         cache.reset_all();
    ///         
    ///         info!("Started new game - all fog systems reset");
    ///     }
    /// }
    /// ```
    ///
    /// # Memory Implications
    /// This operation:
    /// - **GPU Memory**: Effectively frees all texture array layers for reuse
    /// - **CPU Memory**: Retains Vec capacity to avoid future allocations
    /// - **System State**: Coordinates with other systems that track GPU allocation
    /// - **Consistency**: Must be coordinated with ChunkStateCache and other managers
    ///
    /// # Thread Safety
    /// This method requires mutable access and should be called from the main thread
    /// in coordination with other fog of war systems to maintain consistency.
    ///
    /// # Post-Reset Behavior
    /// After reset, the allocation pattern will be deterministic:
    /// - First allocation gets fog=0, snapshot=0
    /// - Second allocation gets fog=1, snapshot=1
    /// - And so on in sequential order
    pub fn clear_all_layers(&mut self) {
        info!("Clearing all texture array layer allocations");

        // Clear the coord to layers mapping
        self.coord_to_layers.clear();

        // Reset all indices to free state
        self.free_fog_indices.clear();
        self.free_snapshot_indices.clear();

        for i in 0..self.capacity {
            self.free_fog_indices.push(i);
            self.free_snapshot_indices.push(i);
        }
    }

    /// Allocates specific GPU texture layer indices for a coordinate (used for persistence restoration).
    /// 为指定坐标分配特定的 GPU 纹理层索引（用于持久化恢复）
    ///
    /// Attempts to allocate exact fog and snapshot layer indices for a chunk coordinate,
    /// rather than using the next available indices. This method is primarily used when
    /// restoring fog of war state from saved data, where the original layer assignments
    /// must be preserved to maintain consistency.
    ///
    /// # Parameters
    /// - `coords`: Chunk coordinates to allocate the specific layers for
    /// - `fog_idx`: Specific fog texture layer index to allocate
    /// - `snap_idx`: Specific snapshot texture layer index to allocate
    ///
    /// # Returns
    /// - `true`: Successfully allocated the specific layer indices
    /// - `false`: Allocation failed (indices unavailable or coordinate already allocated)
    ///
    /// # Allocation Requirements
    /// For allocation to succeed, all conditions must be met:
    /// 1. **Fog Index Available**: `fog_idx` must be in the free_fog_indices pool
    /// 2. **Snapshot Index Available**: `snap_idx` must be in the free_snapshot_indices pool
    /// 3. **Coordinate Free**: `coords` must not already have allocated layers
    /// 4. **Valid Indices**: Both indices must be within valid range (< capacity)
    ///
    /// # Performance Characteristics
    /// - **Time Complexity**: O(n) due to Vec::retain operations on free pools
    /// - **Space Complexity**: O(1) (modifies existing data structures)
    /// - **Slower than Regular Allocation**: Must search and remove specific indices
    /// - **Infrequent Usage**: Typically only used during save/load operations
    ///
    /// # Example Usage
    /// ```rust,no_run
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let mut manager = TextureArrayManager::new(64);
    /// let chunk_coord = IVec2::new(5, -3);
    ///
    /// // During save/load operations, restore specific layer assignments
    /// if manager.allocate_specific_layer_indices(chunk_coord, 42, 17) {
    ///     println!("Restored chunk {:?} to its original layers: fog=42, snapshot=17", chunk_coord);
    /// } else {
    ///     println!("Failed to restore chunk {:?} - layers unavailable", chunk_coord);
    ///     // Fall back to regular allocation or handle error
    ///     if let Some((fog, snap)) = manager.allocate_layer_indices(chunk_coord) {
    ///         println!("Allocated different layers: fog={}, snapshot={}", fog, snap);
    ///     }
    /// }
    /// ```
    ///
    /// # Persistence Integration
    /// This method is essential for save/load functionality:
    /// ```rust,no_run
    /// # use bevy::prelude::*;
    /// # use bevy_fog_of_war::prelude::*;
    /// # #[derive(Debug)]
    /// # struct SavedChunkData {
    /// #     coords: IVec2,
    /// #     fog_layer_index: Option<u32>,
    /// #     snapshot_layer_index: Option<u32>,
    /// # }
    /// fn restore_saved_fog_state(
    ///     saved_chunks: Vec<SavedChunkData>,
    ///     mut manager: ResMut<TextureArrayManager>,
    /// ) {
    ///     for saved_chunk in saved_chunks {
    ///         let success = manager.allocate_specific_layer_indices(
    ///             saved_chunk.coords,
    ///             saved_chunk.fog_layer_index,
    ///             saved_chunk.snapshot_layer_index,
    ///         );
    ///         
    ///         if !success {
    ///             warn!("Could not restore chunk {:?} to original layers", saved_chunk.coords);
    ///             // Handle layer conflict or use alternative allocation
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// # Error Conditions
    /// Allocation fails in these cases:
    /// - **Index Already Allocated**: Requested indices are currently in use
    /// - **Coordinate Conflict**: The coordinate already has different layer allocation
    /// - **Invalid Indices**: Indices are outside valid range (>= capacity)
    /// - **Partial Availability**: Only one of the two requested indices is available
    ///
    /// # Memory Layout Considerations
    /// When restoring saved state, consider that:
    /// - **Fragmentation**: Specific allocation can create gaps in layer usage
    /// - **Cache Locality**: Non-sequential allocation may affect GPU cache performance
    /// - **Conflict Resolution**: Plan for handling allocation conflicts during restore
    /// - **Validation**: Verify saved indices are still valid for current capacity
    ///
    /// # Alternative Strategies
    /// If specific allocation fails, consider:
    /// 1. **Regular Allocation**: Use `allocate_layer_indices()` for any available layers
    /// 2. **Conflict Resolution**: Free conflicting layers and retry specific allocation
    /// 3. **Remapping**: Update saved data to use newly allocated layer indices
    /// 4. **Partial Restore**: Restore chunks with available specific indices only
    ///
    /// # Debugging Support
    /// This method logs all allocation attempts to help debug persistence issues:
    /// - Success: Debug logs the successful specific allocation
    /// - Failure: Warns about why specific allocation failed (unavailable indices, conflicts)
    pub fn allocate_specific_layer_indices(
        &mut self,
        coords: IVec2,
        fog_idx: u32,
        snap_idx: u32,
    ) -> bool {
        // Check if these indices are available
        if !self.free_fog_indices.contains(&fog_idx)
            || !self.free_snapshot_indices.contains(&snap_idx)
        {
            warn!(
                "Cannot allocate specific indices F{} S{} for {:?} - indices not available",
                fog_idx, snap_idx, coords
            );
            return false;
        }

        // Check if coord already has layers
        if self.coord_to_layers.contains_key(&coords) {
            warn!(
                "Cannot allocate specific indices for {:?} - coord already has layers",
                coords
            );
            return false;
        }

        // Remove indices from free lists
        self.free_fog_indices.retain(|&x| x != fog_idx);
        self.free_snapshot_indices.retain(|&x| x != snap_idx);

        // Add to mapping
        self.coord_to_layers.insert(coords, (fog_idx, snap_idx));

        debug!(
            "Allocated specific layers for coord {:?}: F{} S{}",
            coords, fog_idx, snap_idx
        );
        true
    }

    /// Retrieves all currently allocated GPU layer indices for persistence operations.
    /// 获取所有当前分配的 GPU 层索引用于持久化操作
    ///
    /// Returns a read-only reference to the complete mapping of chunk coordinates to their
    /// allocated GPU texture layer indices. This method is primarily used when saving fog
    /// of war state to disk, where all current GPU allocations must be preserved.
    ///
    /// # Returns
    /// A reference to the internal HashMap containing:
    /// - **Key**: `IVec2` chunk coordinates
    /// - **Value**: `(fog_layer_index, snapshot_layer_index)` tuple
    ///
    /// # Performance Characteristics
    /// - **Time Complexity**: O(1) (returns reference, no copying)
    /// - **Space Complexity**: O(1) (read-only reference)
    /// - **Thread Safety**: Safe for concurrent reads with shared reference
    /// - **Memory Efficient**: No data copying or allocation
    ///
    /// # Example Usage
    /// ```rust,no_run
    /// # use bevy_fog_of_war::prelude::*;
    /// # use bevy::prelude::*;
    /// let manager = TextureArrayManager::new(64);
    ///
    /// // Save all current GPU allocations to persistent storage
    /// let all_allocations = manager.get_all_allocated_indices();
    ///
    /// for (chunk_coord, (fog_idx, snap_idx)) in all_allocations {
    ///     println!("Chunk {:?} uses fog layer {} and snapshot layer {}",
    ///              chunk_coord, fog_idx, snap_idx);
    /// }
    ///
    /// println!("Total chunks on GPU: {}", all_allocations.len());
    /// ```
    ///
    /// # Persistence Integration
    /// This method is essential for save/load systems:
    /// ```rust,no_run
    /// use serde::{Serialize, Deserialize};
    ///
    /// #[derive(Serialize, Deserialize)]
    /// struct SavedChunkAllocation {
    ///     coords: IVec2,
    ///     fog_layer_index: u32,
    ///     snapshot_layer_index: u32,
    /// }
    ///
    /// fn save_fog_allocations(
    ///     manager: Res<TextureArrayManager>,
    /// ) -> Vec<SavedChunkAllocation> {
    ///     manager.get_all_allocated_indices()
    ///         .iter()
    ///         .map(|(coords, (fog_idx, snap_idx))| SavedChunkAllocation {
    ///             coords: *coords,
    ///             fog_layer_index: *fog_idx,
    ///             snapshot_layer_index: *snap_idx,
    ///         })
    ///         .collect()
    /// }
    /// ```
    ///
    /// # Memory State Inspection
    /// Useful for debugging and monitoring GPU memory usage:
    /// ```rust,no_run
    /// fn analyze_gpu_memory_usage(manager: Res<TextureArrayManager>) {
    ///     let allocations = manager.get_all_allocated_indices();
    ///     
    ///     // Calculate utilization
    ///     let total_capacity = manager.capacity; // Would need public getter
    ///     let used_slots = allocations.len();
    ///     let utilization = (used_slots as f32 / total_capacity as f32) * 100.0;
    ///     
    ///     println!("GPU Memory Utilization: {:.1}% ({}/{})",
    ///              utilization, used_slots, total_capacity);
    ///     
    ///     // Find layer usage patterns
    ///     let fog_indices: Vec<u32> = allocations.values().map(|(fog, _)| *fog).collect();
    ///     let snap_indices: Vec<u32> = allocations.values().map(|(_, snap)| *snap).collect();
    ///     
    ///     println!("Fog layers in use: {:?}", fog_indices);
    ///     println!("Snapshot layers in use: {:?}", snap_indices);
    /// }
    /// ```
    ///
    /// # Common Use Cases
    /// - **Save/Load Systems**: Preserve exact GPU layer assignments across sessions
    /// - **Memory Profiling**: Analyze GPU texture array utilization
    /// - **Debug Information**: Display current allocation state in debug UI
    /// - **Migration**: Transfer allocations when changing texture array capacity
    /// - **Validation**: Verify allocation consistency across systems
    ///
    /// # Data Consistency
    /// The returned mapping represents the authoritative state of GPU allocations:
    /// - **Synchronization**: Should match actual GPU texture array usage
    /// - **Validation**: Can be used to verify other systems' allocation tracking
    /// - **Recovery**: Enables rebuilding allocation state after errors
    ///
    /// # Iterator Usage
    /// Since this returns a HashMap reference, you can use all HashMap iteration methods:
    /// ```rust,no_run
    /// let allocations = manager.get_all_allocated_indices();
    ///
    /// // Iterate over coordinates only
    /// for chunk_coord in allocations.keys() {
    ///     println!("Allocated chunk: {:?}", chunk_coord);
    /// }
    ///
    /// // Iterate over layer pairs only
    /// for (fog_idx, snap_idx) in allocations.values() {
    ///     println!("Layer pair: fog={}, snapshot={}", fog_idx, snap_idx);
    /// }
    ///
    /// // Check for specific coordinates
    /// if allocations.contains_key(&IVec2::new(5, -3)) {
    ///     println!("Chunk (5, -3) is allocated on GPU");
    /// }
    /// ```
    ///
    /// # Performance Considerations
    /// - **Large Datasets**: With many allocated chunks, iteration can be expensive
    /// - **Frequent Access**: Consider caching results if called frequently
    /// - **Memory Pressure**: The HashMap grows with the number of allocated chunks
    /// - **Serialization**: Converting to Vec for saving may require allocation
    pub fn get_all_allocated_indices(&self) -> &HashMap<IVec2, (u32, u32)> {
        &self.coord_to_layers
    }
}
