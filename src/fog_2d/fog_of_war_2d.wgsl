struct FogOfWarScreen {
    screen_size: vec2<f32>,
    camera_position: vec2<f32>,
    chunk_size: f32,
}

const DEBUG = true;

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
    
    // 修改环形缓存索引计算方式与Rust代码一致
    let ring_x = (relative_x + buffer_width) % buffer_width;
    let ring_y = (relative_y + buffer_height) % buffer_height;
    
    // 直接使用取模后的结果（Rust代码使用rem_euclid保证非负）
    let chunk_index = ring_y * buffer_width + ring_x; // 行优先排列
    
    // 计算块内的局部坐标，y轴翻转以匹配WGSL坐标系
    let local_x = i32(world_pos.x - (f32(chunk_x) * chunk_size));
    let local_y = i32(chunk_size) - 1 - i32(world_pos.y - (f32(chunk_y) * chunk_size));
    
    return vec3<i32>(local_x, local_y, chunk_index);
}

// 修改视野判断逻辑与Rust代码同步
fn is_chunk_in_view(chunk_index: i32) -> bool {
    let chunk_size = screen_size_uniform.chunk_size;
    
    // 计算视口可容纳的块数量（不含padding）
    let view_width = ceil(screen_size_uniform.screen_size.x / chunk_size);
    let view_height = ceil(screen_size_uniform.screen_size.y / chunk_size);
    
    // 修改为 +1 保持对称padding（与Rust代码同步）
    let max_x = i32(view_width) + 1;
    let max_y = i32(view_height) + 1;
    
    // 修改为与Rust代码相同的判断条件
    let buffer_width = i32(view_width) + 2;
    let buffer_height = i32(view_height) + 2;
    
    // 根据环形缓存中的相对位置判断
    let rel_chunk_x = chunk_index % buffer_width;
    let rel_chunk_y = chunk_index / buffer_width;
    
    // 判断是否在有效范围内（包含1个块的边界缓冲）
    return rel_chunk_x >= 0 && rel_chunk_x <= max_x &&
           rel_chunk_y >= 0 && rel_chunk_y <= max_y;
}

// 判断点阵数字中的某个点是否应该被渲染
fn should_render_dot(digit: i32, x: i32, y: i32) -> bool {
    // 5x7点阵数字模板定义（0-9）
    switch digit {
        case 0: {
            return (x == 0 && y > 0 && y < 6) ||  // 左边
                   (x == 4 && y > 0 && y < 6) ||  // 右边
                   (y == 0 && x > 0 && x < 4) ||  // 上边
                   (y == 6 && x > 0 && x < 4);    // 下边
        }
        case 1: {
            return x == 2 || (y == 6 && x >= 1 && x <= 3);
        }
        case 2: {
            return (y == 0 && x > 0 && x < 4) ||  // 上边
                   (y == 3 && x > 0 && x < 4) ||  // 中间
                   (y == 6 && x > 0 && x < 4) ||  // 下边
                   (x == 4 && y > 0 && y < 3) ||  // 右上
                   (x == 0 && y > 3 && y < 6);    // 左下
        }
        case 3: {
            return (y == 0 && x > 0 && x < 4) ||
                   (y == 3 && x > 0 && x < 4) ||
                   (y == 6 && x > 0 && x < 4) ||
                   (x == 4 && y != 0 && y != 3 && y != 6);
        }
        case 4: {
            return (x == 4) ||
                   (x == 0 && y < 4) ||
                   (y == 3);
        }
        case 5: {
            return (y == 0 && x > 0 && x < 4) ||
                   (y == 3 && x > 0 && x < 4) ||
                   (y == 6 && x > 0 && x < 4) ||
                   (x == 0 && y > 0 && y < 3) ||
                   (x == 4 && y > 3 && y < 6);
        }
        case 6: {
            return (y == 0 && x > 0 && x < 4) ||
                   (y == 3 && x > 0 && x < 4) ||
                   (y == 6 && x > 0 && x < 4) ||
                   (x == 0 && y > 0 && y < 6) ||
                   (x == 4 && y > 3 && y < 6);
        }
        case 7: {
            return (y == 0) ||
                   (x == 4);
        }
        case 8: {
            return (y == 0 && x > 0 && x < 4) ||
                   (y == 3 && x > 0 && x < 4) ||
                   (y == 6 && x > 0 && x < 4) ||
                   (x == 0 && y != 0 && y != 3 && y != 6) ||
                   (x == 4 && y != 0 && y != 3 && y != 6);
        }
        case 9: {
            return (y == 0 && x > 0 && x < 4) ||
                   (y == 3 && x > 0 && x < 4) ||
                   (y == 6 && x > 0 && x < 4) ||
                   (x == 0 && y > 0 && y < 3) ||
                   (x == 4 && y != 3 && y != 6);
        }
        default: {
            return false;
        }
    }
}

// 渲染数字函数
fn render_number(number: i32, local_pos: vec2<i32>, dot_size: f32) -> bool {
    // 提取十位和个位数字
    let tens = number / 10;
    let ones = number % 10;
    
    // 计算点阵的基准位置（左上角偏移2个点大小）
    let base_x = 8.0;
    let base_y = 8.0;
    
    // 计算当前像素在点阵中的位置
    let dot_x = i32(floor((f32(local_pos.x) - base_x) / dot_size));
    // 翻转Y轴以匹配点阵数字的定义
    let dot_y = 6 - i32(floor((f32(local_pos.y) - base_y) / dot_size));
    
    // 检查是否在点阵范围内
    if (dot_y >= 0 && dot_y < 7) {
        // 检查十位数字
        if (tens > 0 && dot_x >= 0 && dot_x < 5) {
            return should_render_dot(tens, dot_x, dot_y);
        }
        // 检查个位数字（向右偏移6个点宽度）
        if (dot_x >= 6 && dot_x < 11) {
            return should_render_dot(ones, dot_x - 6, dot_y);
        }
    }
    
    return false;
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
        screen_pos.y + screen_size_uniform.camera_position.y + screen_size_uniform.screen_size.y * 0.5
    );
    
    // Get chunk coordinates and array index
    let chunk_coords = get_chunk_coords(world_pos);
    let local_pos = vec2<i32>(chunk_coords.xy);
    let chunk_index = chunk_coords.z;
    
    // Debug visualization for chunks
    if DEBUG {
        let chunk_size = screen_size_uniform.chunk_size;
        let local_x_norm = f32(local_pos.x) / chunk_size;
        let local_y_norm = f32(local_pos.y) / chunk_size;
        
        // Draw chunk borders
        if (local_x_norm < 0.005 || local_x_norm > 0.995 || 
            local_y_norm < 0.005 || local_y_norm > 0.995) {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0);  // 红色边界
        }

        // 渲染chunk index数字
        let dot_size = chunk_size / 50.0; // 点的大小
        if (render_number(chunk_index, local_pos, dot_size)) {
            return vec4<f32>(0.0, 1.0, 0.0, 1.0); // 绿色数字
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