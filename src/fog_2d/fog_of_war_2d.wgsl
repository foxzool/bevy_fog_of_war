struct FogOfWarScreen {
    screen_size: vec2<f32>,
    camera_position: vec2<f32>,
    chunk_size: f32,
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
    
    // 考虑padding，计算左上角的chunk坐标
    let top_left_chunk_x = camera_chunk_x - 2;
    let top_left_chunk_y = camera_chunk_y - 2;
    
    // 计算相对于视口左上角的坐标
    let relative_x = chunk_x - top_left_chunk_x;
    let relative_y = chunk_y - top_left_chunk_y;
    
    // 计算每行的块数（视口宽度 + padding）
    let chunks_per_row = i32(ceil(screen_size_uniform.screen_size.x / chunk_size)) + 3;
    
    // 计算块索引
    let chunk_index = relative_y * chunks_per_row + relative_x;
    
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
    
    // 计算最大有效块索引（考虑3个块的padding）
    let max_x = i32(view_width) + 2;
    let max_y = i32(view_height) + 2;
    
    // 将块索引转换为二维坐标
    let chunks_per_row = i32(view_width) + 3;
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