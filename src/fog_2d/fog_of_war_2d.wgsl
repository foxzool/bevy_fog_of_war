struct FogOfWarScreen {
    screen_size: vec2<f32>,
    camera_position: vec2<f32>,
    chunk_size: u32,
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
    let chunk_size_f32 = f32(screen_size_uniform.chunk_size);
    let chunk_x = i32(floor(world_pos.x / chunk_size_f32));
    let chunk_y = i32(floor(world_pos.y / chunk_size_f32));
    
    // Calculate visible area bounds in chunks
    let half_width = screen_size_uniform.screen_size.x * 0.5;
    let half_height = screen_size_uniform.screen_size.y * 0.5;
    let min_x = i32(floor((screen_size_uniform.camera_position.x - half_width) / chunk_size_f32)) - 1;
    let max_x = i32(floor((screen_size_uniform.camera_position.x + half_width) / chunk_size_f32)) + 1;
    let min_y = i32(floor((screen_size_uniform.camera_position.y - half_height) / chunk_size_f32)) - 1;
    let max_y = i32(floor((screen_size_uniform.camera_position.y + half_height) / chunk_size_f32)) + 1;
    
    let chunks_per_side = max_x - min_x + 1;
    let chunk_index = (chunk_y - min_y) * chunks_per_side + (chunk_x - min_x);
    
    let local_x = i32(world_pos.x - (f32(chunk_x) * chunk_size_f32));
    let local_y = i32(world_pos.y - (f32(chunk_y) * chunk_size_f32));
    
    return vec3<i32>(local_x, local_y, chunk_index);
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
        chunk_index >= 0 && chunk_index < i32(screen_size_uniform.screen_size.x * screen_size_uniform.screen_size.y)) {
        
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