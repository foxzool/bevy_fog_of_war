#import bevy_render::view::View

const DEBUG = true;


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
var<uniform> view: View;

@group(0) @binding(1)
var<uniform> settings: FogOfWarSettings;

@group(0) @binding(2)
var<storage> sights: array<FogSight2DUniform>;

@group(0) @binding(3)
var explored_texture: texture_storage_2d_array<r8unorm, read_write>;

@group(0) @binding(4)
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

fn get_world_pos(pixel_pos: vec2<f32>) -> vec2<f32> {
    // 使用现有的坐标转换函数替代直接计算
    let ndc = frag_coord_to_ndc(vec4(pixel_pos, 0.0, 1.0));
    let world_pos = position_ndc_to_world(ndc);
    return world_pos.xy;
}

// 通过屏幕像素，换算chunk锚点所在的屏幕像素
fn get_chunk_anchor(pixel_pos: vec2<f32>) -> vec2<f32> {
    let chunk_size = screen_size_uniform.chunk_size;
    let world_pos = get_world_pos(pixel_pos);
    
    // 计算块坐标
    let chunk_x = i32(floor(world_pos.x / chunk_size));
    let chunk_y = i32(floor(world_pos.y / chunk_size));
    
    // 返回世界坐标
    let chunk_world_pos = vec3<f32>(f32(chunk_x) * chunk_size, f32(chunk_y) * chunk_size, 0.0);
    return ndc_to_frag_coord(position_world_to_ndc(chunk_world_pos).xy);
}

fn get_ring_buffer_position(pixel_pos: vec2<f32>) -> vec2<i32> {
    let chunk_indices = get_chunk_indices(pixel_pos);
    let chunk_x = chunk_indices.x;
    let chunk_y = chunk_indices.y;
    
    // 通过view矩阵获取相机位置（世界坐标）
    let camera_pos = position_ndc_to_world(vec3(0.0)).xy;
    let camera_chunk_x = i32(floor(camera_pos.x / screen_size_uniform.chunk_size));
    let camera_chunk_y = i32(floor(camera_pos.y / screen_size_uniform.chunk_size));
    
    // 通过viewport获取实际屏幕尺寸
    let view_size = view.viewport.zw;
    let view_width = i32(ceil(view_size.x / screen_size_uniform.chunk_size));
    let view_height = i32(ceil(view_size.y / screen_size_uniform.chunk_size));
    let buffer_width = view_width + 2;
    let buffer_height = view_height + 2;
    
    // 计算视口的左上角chunk坐标
    let viewport_start_x = camera_chunk_x - buffer_width / 2;
    let viewport_start_y = camera_chunk_y + buffer_height / 2;
    
    // 计算chunk相对于视口左上角的偏移
    let relative_x = chunk_x - viewport_start_x;
    let relative_y = viewport_start_y - chunk_y;
    
    return vec2<i32>(relative_x, relative_y);
}

// 修改视野判断逻辑与Rust代码同步
fn is_chunk_in_view(chunk_index: i32) -> bool {
    let chunk_size = screen_size_uniform.chunk_size;
    
    // 计算视口可容纳的块数量（不含padding）
    let view_width = ceil(view.viewport.zw.x / chunk_size);
    let view_height = ceil(view.viewport.zw.y / chunk_size);
    
    // 修改为与Rust代码相同的判断条件
    let buffer_width = i32(view_width) + 2;
    let buffer_height = i32(view_height) + 2;
    
    // 根据环形缓存中的相对位置判断
    let rel_chunk_x = chunk_index % buffer_width;
    let rel_chunk_y = chunk_index / buffer_width;
    
    // 判断是否在有效范围内（包含1个块的边界缓冲）
    return rel_chunk_x >= 0 && rel_chunk_x < buffer_width &&
           rel_chunk_y >= 0 && rel_chunk_y < buffer_height;
}

// 判断点阵数字中的某个点是否应该被渲染
fn should_render_dot(digit: i32, x: i32, y: i32) -> bool {
    let y_inv = 6 - y; // 反转Y坐标系统
    // 5x7点阵数字模板定义（0-9）
    switch digit {
        case 0: {
            return (x == 0 && y_inv > 0 && y_inv < 6) ||  // 左边
                   (x == 4 && y_inv > 0 && y_inv < 6) ||  // 右边
                   (y_inv == 0 && x > 0 && x < 4) ||  // 上边
                   (y_inv == 6 && x > 0 && x < 4);    // 下边
        }
        case 1: {
            return x == 2 || (y_inv == 6 && x >= 1 && x <= 3);
        }
        case 2: {
            return (y_inv == 0 && x > 0 && x < 4) ||  // 上边
                   (y_inv == 3 && x > 0 && x < 4) ||  // 中间
                   (y_inv == 6 && x > 0 && x < 4) ||  // 下边
                   (x == 4 && y_inv > 0 && y_inv < 3) ||  // 右上
                   (x == 0 && y_inv > 3 && y_inv < 6);    // 左下
        }
        case 3: {
            return (y_inv == 0 && x > 0 && x < 4) ||
                   (y_inv == 3 && x > 0 && x < 4) ||
                   (y_inv == 6 && x > 0 && x < 4) ||
                   (x == 4 && y_inv != 0 && y_inv != 3 && y_inv != 6);
        }
        case 4: {
            return (x == 4) ||
                   (x == 0 && y_inv < 4) ||
                   (y_inv == 3);
        }
        case 5: {
            return (y_inv == 0 && x > 0 && x < 4) ||
                   (y_inv == 3 && x > 0 && x < 4) ||
                   (y_inv == 6 && x > 0 && x < 4) ||
                   (x == 0 && y_inv > 0 && y_inv < 3) ||
                   (x == 4 && y_inv > 3 && y_inv < 6);
        }
        case 6: {
            return (y_inv == 0 && x > 0 && x < 4) ||
                   (y_inv == 3 && x > 0 && x < 4) ||
                   (y_inv == 6 && x > 0 && x < 4) ||
                   (x == 0 && y_inv > 0 && y_inv < 6) ||
                   (x == 4 && y_inv > 3 && y_inv < 6);
        }
        case 7: {
            return (y_inv == 0) ||
                   (x == 4);
        }
        case 8: {
            return (y_inv == 0 && x > 0 && x < 4) ||
                   (y_inv == 3 && x > 0 && x < 4) ||
                   (y_inv == 6 && x > 0 && x < 4) ||
                   (x == 0 && y_inv != 0 && y_inv != 3 && y_inv != 6) ||
                   (x == 4 && y_inv != 0 && y_inv != 3 && y_inv != 6);
        }
        case 9: {
            return (y_inv == 0 && x > 0 && x < 4) ||
                   (y_inv == 3 && x > 0 && x < 4) ||
                   (y_inv == 6 && x > 0 && x < 4) ||
                   (x == 0 && y_inv > 0 && y_inv < 3) ||
                   (x == 4 && y_inv != 3 && y_inv != 6);
        }
        default: {
            return false;
        }
    }
}

// 添加渲染负号的函数
fn should_render_minus(x: i32, y: i32) -> bool {
    let y_inv = 6 - y; // 反转Y坐标系统
    return y_inv == 3 && x >= 0 && x < 3;
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
fn render_number_at_position(number: i32, local_pos: vec2<f32>, base_x: f32, base_y: f32, dot_size: f32) -> bool {
    let is_negative = number < 0;
    let num_digits = get_num_digits(number);
    
    // 计算当前像素在点阵中的位置，y坐标需要翻转
    let dot_x = i32(floor((local_pos.x - base_x) / dot_size));
    let dot_y = 6 - i32(floor((local_pos.y - base_y) / dot_size)); // 翻转y坐标
    
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

//
//// 返回像素点在chunk内部的偏移值， 这里是y朝下
//fn get_local_coords(pixel_pos: vec2<f32>) -> vec2<f32> {
//    let chunk_indices = get_chunk_indices(pixel_pos);
//    let chunk_x = f32(chunk_indices.x);
//    let chunk_y = f32(chunk_indices.y);
//
//    let chunk_size = screen_size_uniform.chunk_size;
//
//    // 获取世界坐标并添加微小偏移以避免边界问题
//    let world_pos = get_world_pos(pixel_pos) + vec2<f32>(0.001);
//
//    // 使用floor进行chunk坐标计算
//    let chunk_start_x = chunk_x * chunk_size;
//    let chunk_start_y = chunk_y * chunk_size;
//
//    // 计算相对于chunk起点的偏移
//    let local_x = world_pos.x - chunk_start_x;
//    let local_y = world_pos.y - chunk_start_y;
//
//    // 确保坐标在chunk大小范围内
//    let clamped_x = clamp(local_x, 0.0, chunk_size - 1.0);
//    let clamped_y = clamp(local_y, 0.0, chunk_size - 1.0);
//
//    return vec2<f32>(clamped_x, clamped_y);
//}

fn get_chunk_index_from_pixel(pixel_pos: vec2<f32>) -> i32 {
    // 获取环形缓冲区位置
    let ring_pos = get_ring_buffer_position(pixel_pos);
    
    // 计算缓冲区宽度和高度
    let view_width = i32(ceil(view.viewport.zw.x / screen_size_uniform.chunk_size));
    let view_height = i32(ceil(view.viewport.zw.y / screen_size_uniform.chunk_size));
    let buffer_width = view_width + 2;
    let buffer_height = view_height + 2;
    
    // 使用calculate_chunk_index计算最终的chunk索引
    return calculate_chunk_index(ring_pos.x, ring_pos.y, buffer_width, buffer_height);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let ndc_position = frag_coord_to_ndc(frag_coord);
    let world_position = position_ndc_to_world(ndc_position);

    var visibility = 0.0;
    
    // 计算可见性（使用世界坐标）
    for (var i = 0u; i < arrayLength(&sights); i++) {
        let sight = sights[i];
        // 直接使用世界坐标计算距离
        let dist = distance(world_position.xy, sight.position);
        if (dist < sight.radius) {
            visibility = max(visibility, 1.0 - smoothstep(sight.radius - settings.fade_width, sight.radius, dist));
        }
    }
    
    // 获取各种坐标
    let pixel_pos = vec2<f32>(frag_coord.x, frag_coord.y);
//    let chunk_anchor = get_chunk_anchor(pixel_pos);
//    let local_pos = get_local_coords(pixel_pos);
    // let ring_pos = get_ring_buffer_position(pixel_pos);
    
    // 计算chunk索引
    let chunk_index = get_chunk_index_from_pixel(pixel_pos);
    let chunk_size = screen_size_uniform.chunk_size;
    
    // 修改后的调试可视化
    if DEBUG {
        if chunk_index == 17 {
            // 获取世界坐标并计算局部坐标
            let world_pos = get_world_pos(pixel_pos);
            let chunk_x = floor(world_pos.x / chunk_size);
            let chunk_y = floor(world_pos.y / chunk_size);
            
            // 计算在chunk内的局部坐标
            let local_x = world_pos.x - chunk_x * chunk_size;
            let local_y = world_pos.y - chunk_y * chunk_size;
            
            // 绘制四周边框（1像素宽度）
            let border = 1.0;
            if local_x < border || local_x > chunk_size - border ||
               local_y < border || local_y > chunk_size - border {
                return vec4<f32>(1.0, 0.0, 0.0, 1.0);
            }
            
            // 新增：在chunk中心显示索引数字
            let number_pos = vec2<f32>(chunk_x * chunk_size + 5.0, chunk_y * chunk_size + 5.0);
            let dot_size = 2.0;
            if render_number_at_position(
                chunk_index, 
                world_pos.xy,
                number_pos.x,
                number_pos.y,
                dot_size
            ) {
                return vec4<f32>(0.0, 1.0, 0.0, 1.0); // 绿色数字
            }
        }
    }


    
    // 检查是否在有效的chunk范围内
//    if (is_chunk_in_view(chunk_index)) {
//        let local_pos_i32 = vec2<i32>(local_pos);
//        let explored = textureLoad(explored_texture, local_pos_i32, chunk_index);
//        let new_explored = max(explored.r, visibility);
//
//        // 计算到chunk边界的距离
//        let edge_dist_x = min(local_pos.x, chunk_size - local_pos.x);
//        let edge_dist_y = min(local_pos.y, chunk_size - local_pos.y);
//        let edge_dist = min(edge_dist_x, edge_dist_y);
//
//        // 在边界处轻微扩展可见区域
//        let edge_blend = smoothstep(0.0, 1.0, edge_dist);
//        let adjusted_explored = mix(new_explored + 0.001, new_explored, edge_blend);
//
//        // 存储结果
//        textureStore(explored_texture, local_pos_i32, chunk_index, vec4<f32>(adjusted_explored));
//
//        let final_visibility = max(visibility, adjusted_explored * settings.explored_alpha);
//        return mix(settings.fog_color, vec4<f32>(0.0), final_visibility);
//    }
    
    return mix(settings.fog_color, vec4<f32>(0.0), visibility);
}


/// World space:
/// +y is up

/// View space:
/// -z is forward, +x is right, +y is up
/// Forward is from the camera position into the scene.
/// (0.0, 0.0, -1.0) is linear distance of 1.0 in front of the camera's view relative to the camera's rotation
/// (0.0, 1.0, 0.0) is linear distance of 1.0 above the camera's view relative to the camera's rotation

/// NDC (normalized device coordinate):
/// https://www.w3.org/TR/webgpu/#coordinate-systems
/// (-1.0, -1.0) in NDC is located at the bottom-left corner of NDC
/// (1.0, 1.0) in NDC is located at the top-right corner of NDC
/// Z is depth where:
///    1.0 is near clipping plane
///    Perspective projection: 0.0 is inf far away
///    Orthographic projection: 0.0 is far clipping plane

/// UV space:
/// 0.0, 0.0 is the top left
/// 1.0, 1.0 is the bottom right


// -----------------
// TO WORLD --------
// -----------------

/// Convert a view space position to world space
fn position_view_to_world(view_pos: vec3<f32>) -> vec3<f32> {
    let world_pos = view.world_from_view * vec4(view_pos, 1.0);
    return world_pos.xyz;
}

/// Convert a clip space position to world space
fn position_clip_to_world(clip_pos: vec4<f32>) -> vec3<f32> {
    let world_pos = view.world_from_clip * clip_pos;
    return world_pos.xyz;
}

/// Convert a ndc space position to world space
fn position_ndc_to_world(ndc_pos: vec3<f32>) -> vec3<f32> {
    let world_pos = view.world_from_clip * vec4(ndc_pos, 1.0);
    return world_pos.xyz / world_pos.w;
}

/// Convert a view space direction to world space
fn direction_view_to_world(view_dir: vec3<f32>) -> vec3<f32> {
    let world_dir = view.world_from_view * vec4(view_dir, 0.0);
    return world_dir.xyz;
}

/// Convert a clip space direction to world space
fn direction_clip_to_world(clip_dir: vec4<f32>) -> vec3<f32> {
    let world_dir = view.world_from_clip * clip_dir;
    return world_dir.xyz;
}

// -----------------
// TO VIEW ---------
// -----------------

/// Convert a world space position to view space
fn position_world_to_view(world_pos: vec3<f32>) -> vec3<f32> {
    let view_pos = view.view_from_world * vec4(world_pos, 1.0);
    return view_pos.xyz;
}

/// Convert a clip space position to view space
fn position_clip_to_view(clip_pos: vec4<f32>) -> vec3<f32> {
    let view_pos = view.view_from_clip * clip_pos;
    return view_pos.xyz;
}

/// Convert a ndc space position to view space
fn position_ndc_to_view(ndc_pos: vec3<f32>) -> vec3<f32> {
    let view_pos = view.view_from_clip * vec4(ndc_pos, 1.0);
    return view_pos.xyz / view_pos.w;
}

/// Convert a world space direction to view space
fn direction_world_to_view(world_dir: vec3<f32>) -> vec3<f32> {
    let view_dir = view.view_from_world * vec4(world_dir, 0.0);
    return view_dir.xyz;
}

/// Convert a clip space direction to view space
fn direction_clip_to_view(clip_dir: vec4<f32>) -> vec3<f32> {
    let view_dir = view.view_from_clip * clip_dir;
    return view_dir.xyz;
}

// -----------------
// TO CLIP ---------
// -----------------

/// Convert a world space position to clip space
fn position_world_to_clip(world_pos: vec3<f32>) -> vec4<f32> {
    let clip_pos = view.clip_from_world * vec4(world_pos, 1.0);
    return clip_pos;
}

/// Convert a view space position to clip space
fn position_view_to_clip(view_pos: vec3<f32>) -> vec4<f32> {
    let clip_pos = view.clip_from_view * vec4(view_pos, 1.0);
    return clip_pos;
}

/// Convert a world space direction to clip space
fn direction_world_to_clip(world_dir: vec3<f32>) -> vec4<f32> {
    let clip_dir = view.clip_from_world * vec4(world_dir, 0.0);
    return clip_dir;
}

/// Convert a view space direction to clip space
fn direction_view_to_clip(view_dir: vec3<f32>) -> vec4<f32> {
    let clip_dir = view.clip_from_view * vec4(view_dir, 0.0);
    return clip_dir;
}

// -----------------
// TO NDC ----------
// -----------------

/// Convert a world space position to ndc space
fn position_world_to_ndc(world_pos: vec3<f32>) -> vec3<f32> {
    let ndc_pos = view.clip_from_world * vec4(world_pos, 1.0);
    return ndc_pos.xyz / ndc_pos.w;
}

/// Convert a view space position to ndc space
fn position_view_to_ndc(view_pos: vec3<f32>) -> vec3<f32> {
    let ndc_pos = view.clip_from_view * vec4(view_pos, 1.0);
    return ndc_pos.xyz / ndc_pos.w;
}

// -----------------
// DEPTH -----------
// -----------------

/// Retrieve the perspective camera near clipping plane
fn perspective_camera_near() -> f32 {
    return view.clip_from_view[3][2];
}

/// Convert ndc depth to linear view z.
/// Note: Depth values in front of the camera will be negative as -z is forward
fn depth_ndc_to_view_z(ndc_depth: f32) -> f32 {
#ifdef VIEW_PROJECTION_PERSPECTIVE
    return -perspective_camera_near() / ndc_depth;
#else ifdef VIEW_PROJECTION_ORTHOGRAPHIC
    return -(view.clip_from_view[3][2] - ndc_depth) / view.clip_from_view[2][2];
#else
    let view_pos = view.view_from_clip * vec4(0.0, 0.0, ndc_depth, 1.0);
    return view_pos.z / view_pos.w;
#endif
}

/// Convert linear view z to ndc depth.
/// Note: View z input should be negative for values in front of the camera as -z is forward
fn view_z_to_depth_ndc(view_z: f32) -> f32 {
#ifdef VIEW_PROJECTION_PERSPECTIVE
    return -perspective_camera_near() / view_z;
#else ifdef VIEW_PROJECTION_ORTHOGRAPHIC
    return view.clip_from_view[3][2] + view_z * view.clip_from_view[2][2];
#else
    let ndc_pos = view.clip_from_view * vec4(0.0, 0.0, view_z, 1.0);
    return ndc_pos.z / ndc_pos.w;
#endif
}

// -----------------
// UV --------------
// -----------------

/// Convert ndc space xy coordinate [-1.0 .. 1.0] to uv [0.0 .. 1.0]
fn ndc_to_uv(ndc: vec2<f32>) -> vec2<f32> {
    return ndc * vec2(0.5, -0.5) + vec2(0.5);
}

/// Convert uv [0.0 .. 1.0] coordinate to ndc space xy [-1.0 .. 1.0]
fn uv_to_ndc(uv: vec2<f32>) -> vec2<f32> {
    return uv * vec2(2.0, -2.0) + vec2(-1.0, 1.0);
}

/// returns the (0.0, 0.0) .. (1.0, 1.0) position within the viewport for the current render target
/// [0 .. render target viewport size] eg. [(0.0, 0.0) .. (1280.0, 720.0)] to [(0.0, 0.0) .. (1.0, 1.0)]
fn frag_coord_to_uv(frag_coord: vec2<f32>) -> vec2<f32> {
    return (frag_coord - view.viewport.xy) / view.viewport.zw;
}

/// Convert frag coord to ndc
fn frag_coord_to_ndc(frag_coord: vec4<f32>) -> vec3<f32> {
    return vec3(uv_to_ndc(frag_coord_to_uv(frag_coord.xy)), frag_coord.z);
}

/// Convert ndc space xy coordinate [-1.0 .. 1.0] to [0 .. render target
/// viewport size]
fn ndc_to_frag_coord(ndc: vec2<f32>) -> vec2<f32> {
    return ndc_to_uv(ndc) * view.viewport.zw;
}

// 新增函数：从像素坐标直接计算chunk坐标
fn get_chunk_indices(pixel_pos: vec2<f32>) -> vec2<i32> {
    let chunk_size = screen_size_uniform.chunk_size;
    let world_pos = get_world_pos(pixel_pos);
    
    // 添加Y轴翻转（根据Bevy坐标系）
    let adjusted_world_pos = vec2<f32>(world_pos.x, -world_pos.y);
    
    // 使用floor进行块坐标计算
    let chunk_x = i32(floor(adjusted_world_pos.x / chunk_size));
    let chunk_y = i32(floor(adjusted_world_pos.y / chunk_size));
    
    return vec2<i32>(chunk_x, chunk_y);
}
