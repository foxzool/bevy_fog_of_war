struct FogOfWarSettings {
    fog_color: vec4<f32>,
    screen_size: vec2<f32>,
    fade_width: f32,
    explored_alpha: f32,
}

struct FogSight2D {
    position: vec2<f32>,
    radius: f32,
}

@group(0) @binding(0)
var<uniform> settings: FogOfWarSettings;

@group(0) @binding(1)
var<storage> sights: array<FogSight2D>;

@group(0) @binding(2)
var explored_texture: texture_storage_2d<r8unorm, read_write>;

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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Convert UV coordinates to screen coordinates, keeping Y-axis direction consistent with Bevy
    let pixel_pos = vec2<f32>(
        (in.uv.x - 0.5) * settings.screen_size.x,
        (0.5 - in.uv.y) * settings.screen_size.y
    );
    var visibility = 0.0;
    
    // Check current visibility
    for (var i = 0u; i < arrayLength(&sights); i++) {
        let sight = sights[i];
        let dist = distance(pixel_pos, sight.position);
        if (dist < sight.radius) {
            visibility = max(visibility, 1.0 - smoothstep(sight.radius - settings.fade_width, sight.radius, dist));
        }
    }
    
    // Update explored area
    let texture_pos = vec2<i32>(
        i32(in.uv.x * settings.screen_size.x),
        i32(in.uv.y * settings.screen_size.y)
    );
    let explored = textureLoad(explored_texture, texture_pos);
    let new_explored = max(explored.r, visibility);
    textureStore(explored_texture, texture_pos, vec4<f32>(new_explored));
    
    // Blend current visibility with explored area
    let final_visibility = max(visibility, explored.r * settings.explored_alpha);
    
    return mix(settings.fog_color, vec4<f32>(0.0), final_visibility);
}