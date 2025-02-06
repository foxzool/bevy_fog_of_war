struct FogOfWarScreen {
    screen_size: vec2<f32>,
    camera_position: vec2<f32>,
    chunk_size: f32,
    debug: u32 // 0: 关闭, 1: 开启
}

struct FogOfWarSettings {
    fog_color: vec4<f32>,
    fade_width: f32,
    explored_alpha: f32,
}

struct FogSight2DUniform {
    position: vec2<f32>,
    radius: f32,
}

@group(0) @binding(0)
var<uniform> settings: FogOfWarSettings;

@group(0) @binding(1) 
var<storage> sights: array<FogSight2DUniform>;

@group(0) @binding(2)
var explored_texture: texture_storage_2d_array<r8unorm, read_write>;

@group(0) @binding(3)
var<uniform> screen_size_uniform: FogOfWarScreen;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) color: vec4<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(position, 1.0);
    out.uv = position.xy * 0.5 + 0.5;
    return out;
}

fn get_chunk_coords(world_pos: vec2<f32>) -> vec3<i32> {
    let chunk_size = screen_size_uniform.chunk_size;
    
    // 计算世界坐标中的块坐标
    let chunk_x = i32(floor(world_pos.x / chunk_size));
    let chunk_y = i32(floor(world_pos.y / chunk_size));
    
    // 计算相机位置对应的chunk坐标
    let camera_chunk_x = i32(floor(screen_size_uniform.camera_position.x / chunk_size));
    let camera_chunk_y = i32(floor(screen_size_uniform.camera_position.y / chunk_size));
    
    // 计算环形缓存的大小
    let view_width = i32(ceil(screen_size_uniform.screen_size.x / chunk_size));
    let view_height = i32(ceil(screen_size_uniform.screen_size.y / chunk_size));
    let buffer_width = view_width + 2;  // 额外的缓冲区
    let buffer_height = view_height + 2;
    
    // 计算chunk在环形缓存中的相对位置
    let relative_x = chunk_x - (camera_chunk_x - buffer_width / 2);
    let relative_y = chunk_y - (camera_chunk_y - buffer_height / 2);
    
    // 使用取模运算实现环形缓存
    let ring_x = relative_x % buffer_width;
    let ring_y = relative_y % buffer_height;
    
    // 确保结果为正数
    let normalized_x = select(ring_x + buffer_width, ring_x, ring_x >= 0);
    let normalized_y = select(ring_y + buffer_height, ring_y, ring_y >= 0);
    
    // 计算最终的环形缓存索引
    let chunk_index = (normalized_y % buffer_height) * buffer_width + (normalized_x % buffer_width);
    
    // 计算块内的局部坐标
    let local_x = i32(world_pos.x - (f32(chunk_x) * chunk_size));
    let local_y = i32(world_pos.y - (f32(chunk_y) * chunk_size));
    
    return vec3<i32>(local_x, local_y, chunk_index);
}

// 新增视野判断函数
fn is_chunk_in_view(chunk_index: i32) -> bool {
    let chunk_size = screen_size_uniform.chunk_size;
    
    // 计算视口可容纳的块数量（不含padding）
    let view_width = ceil(screen_size_uniform.screen_size.x / chunk_size);
    let view_height = ceil(screen_size_uniform.screen_size.y / chunk_size);
    
    // 修改为 +1 保持对称padding（与Rust代码同步）
    let max_x = i32(view_width) + 1;
    let max_y = i32(view_height) + 1;
    
    // 修改为 +2 保持与chunks_per_row计算一致
    let chunks_per_row = i32(view_width) + 2;
    let rel_chunk_x = chunk_index % chunks_per_row;
    let rel_chunk_y = chunk_index / chunks_per_row;
    
    // 判断是否在有效范围内（包含1个块的边界缓冲）
    return rel_chunk_x >= -1 && rel_chunk_x <= max_x &&
           rel_chunk_y >= -1 && rel_chunk_y <= max_y &&
           chunk_index >= 0;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Convert UV to world space coordinates relative to camera
    let screen_pos = vec2<f32>(
        (in.uv.x - 0.5) * screen_size_uniform.screen_size.x,
        (0.5 - in.uv.y) * screen_size_uniform.screen_size.y
    );
    
    var visibility = 0.0;
    
    // Calculate visibility using screen space coordinates
    for (var i = 0u; i < arrayLength(&sights); i++) {
        let sight = sights[i];
        let dist = distance(screen_pos, sight.position);
        if (dist < sight.radius) {
            visibility = max(visibility, 1.0 - smoothstep(sight.radius - settings.fade_width, sight.radius, dist));
        }
    }
    
    // Convert screen position to world space coordinates for texture sampling
    let world_pos = vec2<f32>(
        screen_pos.x + screen_size_uniform.camera_position.x + screen_size_uniform.screen_size.x * 0.5,
        -screen_pos.y + screen_size_uniform.camera_position.y + screen_size_uniform.screen_size.y * 0.5
    );
    
    // Get chunk coordinates and array index
    let chunk_coords = get_chunk_coords(world_pos);
    let local_pos = vec2<i32>(chunk_coords.xy);
    let chunk_index = chunk_coords.z;
    
    // Debug visualization for chunks
    if (screen_size_uniform.debug == 1u) {
        let chunk_size = screen_size_uniform.chunk_size;
        let local_x_norm = f32(local_pos.x) / chunk_size;
        let local_y_norm = f32(local_pos.y) / chunk_size;
        
        // Draw chunk borders (更细的边界线)
        if (local_x_norm < 0.005 || local_x_norm > 0.995 || 
            local_y_norm < 0.005 || local_y_norm > 0.995) {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0);  // 红色边界
        }
    }
    
    // Only update explored texture if within bounds
    if (local_pos.x >= 0 && local_pos.x < i32(screen_size_uniform.chunk_size) &&
        local_pos.y >= 0 && local_pos.y < i32(screen_size_uniform.chunk_size) &&
        is_chunk_in_view(chunk_index)) {
        
        let explored = textureLoad(explored_texture, local_pos, chunk_index);
        let new_explored = max(explored.r, visibility);
        textureStore(explored_texture, local_pos, chunk_index, vec4<f32>(new_explored));
        
        // Blend current visibility with explored area
        let final_visibility = max(visibility, explored.r * settings.explored_alpha);
        return mix(settings.fog_color, vec4<f32>(0.0), final_visibility);
    }
    
    // If out of bounds, just use current visibility
    return mix(settings.fog_color, vec4<f32>(0.0), visibility);
}