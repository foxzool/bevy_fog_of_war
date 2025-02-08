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

fn calculate_chunk_index(relative_x: i32, relative_y: i32, buffer_width: i32, buffer_height: i32) -> i32 {
    // 环形缓存索引计算
    let ring_x = (relative_x + buffer_width) % buffer_width;
    let ring_y = (relative_y + buffer_height) % buffer_height;
    return ring_y * buffer_width + ring_x;
}

fn get_chunk_coords(uv: vec2<f32>) -> vec3<i32> {
    let chunk_size = screen_size_uniform.chunk_size;
    let screen_pos = vec2<f32>(
        uv.x * screen_size_uniform.screen_size.x,
        (1.0 - uv.y) * screen_size_uniform.screen_size.y  // 翻转Y轴
    );
    
    // 计算世界坐标（Y轴需要反向）
    let world_pos = vec2<f32>(
        screen_pos.x + screen_size_uniform.camera_position.x - screen_size_uniform.screen_size.x * 0.5,
        screen_pos.y - screen_size_uniform.camera_position.y - screen_size_uniform.screen_size.y * 0.5
    );
    
    // 计算块坐标
    let chunk_x = i32(floor(world_pos.x / chunk_size));
    let chunk_y = i32(floor(world_pos.y / chunk_size));
    
    // 计算相机位置对应的chunk坐标
    let camera_chunk_x = i32(floor(screen_size_uniform.camera_position.x / chunk_size));
    let camera_chunk_y = i32(floor(screen_size_uniform.camera_position.y / chunk_size));
    
    // 计算环形缓存的大小
    let view_width = i32(ceil(screen_size_uniform.screen_size.x / chunk_size));
    let view_height = i32(ceil(screen_size_uniform.screen_size.y / chunk_size));
    let buffer_width = view_width + 2;
    let buffer_height = view_height + 2;
    
    // 计算chunk在环形缓存中的相对位置
    let relative_x = chunk_x - (camera_chunk_x - buffer_width / 2);
    let relative_y = chunk_y - (camera_chunk_y - buffer_height / 2);
    
    // 使用新函数计算chunk索引
    let chunk_index = calculate_chunk_index(relative_x, relative_y, buffer_width, buffer_height);
    
    // 计算块内的局部坐标
    let local_x = i32((world_pos.x - f32(chunk_x) * chunk_size));
    let local_y = i32((world_pos.y - f32(chunk_y) * chunk_size));
    
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

// 添加渲染负号的函数
fn should_render_minus(x: i32, y: i32) -> bool {
    return y == 3 && x >= 0 && x < 3;
}

// 获取数字的位数
fn get_num_digits(num: i32) -> i32 {
    var n = abs(num);
    if (n == 0) {
        return 1;
    }
    var count = 0;
    while (n > 0) {
        count += 1;
        n = n / 10;
    }
    return count;
}

// 获取特定位置的数字
fn get_digit_at(num: i32, position: i32) -> i32 {
    var n = abs(num);
    for (var i = 0; i < position; i++) {
        n = n / 10;
    }
    return n % 10;
}

// 修改渲染数字函数以支持任意位数和负数
fn render_number_at_position(number: i32, local_pos: vec2<i32>, base_x: f32, base_y: f32, dot_size: f32) -> bool {
    let is_negative = number < 0;
    let num_digits = get_num_digits(number);
    
    // 计算当前像素在点阵中的位置
    let dot_x = i32(floor((f32(local_pos.x) - base_x) / dot_size));
    let dot_y = i32(floor((f32(local_pos.y) - base_y) / dot_size));
    
    // 检查是否在点阵范围内
    if (dot_y >= 0 && dot_y < 7) {
        // 检查负号
        if (is_negative && dot_x >= 0 && dot_x < 3) {
            return should_render_minus(dot_x, dot_y);
        }
        
        // 计算数字的起始位置（考虑负号的偏移）
        let digit_start_x = select(0, 4, is_negative);
        
        // 遍历每一位数字
        for (var i = 0; i < num_digits; i++) {
            let digit_x = dot_x - (digit_start_x + i * 6);
            if (digit_x >= 0 && digit_x < 5) {
                let digit = get_digit_at(number, num_digits - 1 - i);
                return should_render_dot(digit, digit_x, dot_y);
            }
        }
    }
    
    return false;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let screen_pos = vec2<f32>(
        (in.uv.x * screen_size_uniform.screen_size.x) - screen_size_uniform.screen_size.x * 0.5,
        (in.uv.y * screen_size_uniform.screen_size.y) - screen_size_uniform.screen_size.y * 0.5
    );
    
    var visibility = 0.0;
    
    // 计算可见性
    for (var i = 0u; i < arrayLength(&sights); i++) {
        let sight = sights[i];
        let dist = distance(screen_pos, sight.position);
        if (dist < sight.radius) {
            visibility = max(visibility, 1.0 - smoothstep(sight.radius - settings.fade_width, sight.radius, dist));
        }
    }
    
    // 获取chunk坐标
    let chunk_coords = get_chunk_coords(in.uv);
    let local_pos = vec2<i32>(chunk_coords.xy);
    let chunk_index = chunk_coords.z;
    
    // Debug可视化
    if DEBUG {
        let chunk_size = screen_size_uniform.chunk_size;
        let distance_from_left = f32(local_pos.x);
        let distance_from_top = f32(local_pos.y);
        
        let line_width = 3.0;

        if (chunk_index % 7 == 3) {
            // 左边线（所有chunk统一红色）
            if (distance_from_left < line_width) {
                return vec4<f32>(1.0, 0.0, 0.0, 1.0);
            }
            // 上边线（所有chunk统一绿色）
            if (distance_from_top < line_width) {
                return vec4<f32>(0.0, 1.0, 0.0, 1.0);
            }

            let dot_size = chunk_size / 50.0;
            
            // 计算ring坐标
            let buffer_width = i32(ceil(screen_size_uniform.screen_size.x / chunk_size)) + 2;
            let ring_x = chunk_index % buffer_width;
            let ring_y = chunk_index / buffer_width;
            
            // 渲染chunk索引
            if (render_number_at_position(chunk_index, local_pos, 8.0, 68.0, dot_size)) {
                return vec4<f32>(0.0, 1.0, 0.0, 1.0);
            }
            
            // 渲染ring_x
            if (render_number_at_position(ring_x, local_pos, 8.0, 150.0, dot_size)) {
                return vec4<f32>(0.0, 1.0, 0.0, 1.0);
            }
            
            // 渲染ring_y
            if (render_number_at_position(ring_y, local_pos, 68.0, 150.0, dot_size)) {
                return vec4<f32>(0.0, 1.0, 0.0, 1.0);
            }
        }
    }

    // 更新和返回最终颜色
    if (local_pos.x >= 0 && local_pos.x < i32(screen_size_uniform.chunk_size) &&
        local_pos.y >= 0 && local_pos.y < i32(screen_size_uniform.chunk_size) &&
        is_chunk_in_view(chunk_index)) {
        
        let explored = textureLoad(explored_texture, local_pos, chunk_index);
        let new_explored = max(explored.r, visibility);
        textureStore(explored_texture, local_pos, chunk_index, vec4<f32>(new_explored));
        
        let final_visibility = max(visibility, explored.r * settings.explored_alpha);
        return mix(settings.fog_color, vec4<f32>(0.0), final_visibility);
    }
    
    return mix(settings.fog_color, vec4<f32>(0.0), visibility);
}