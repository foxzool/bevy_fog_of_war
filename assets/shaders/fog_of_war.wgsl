struct FogOfWarSettings {
    color: vec4f,
    screen_size: vec2f,
    fade_width: f32,
};

struct FogSight2D {
    position: vec2f,
    radius: f32,
}

@group(0) @binding(0) var<uniform> settings: FogOfWarSettings;
@group(0) @binding(1) var<storage> sight_points: array<FogSight2D>;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) color: vec4f,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) color: vec4f,
    @location(1) position: vec2f,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4f(model.position, 1.0);
    out.color = model.color;
    out.position = model.position.xy;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let screen_position = vec2f(in.position.x * settings.screen_size.x * 0.5, 
                              in.position.y * settings.screen_size.y * 0.5);
    
    var final_alpha = 1.0;
    
    for(var i = 0u; i < arrayLength(&sight_points); i++) {
        let sight = sight_points[i];
        let sight_pos = sight.position;
        let distance = length(screen_position - sight_pos);
        let outer_radius = sight.radius + settings.fade_width;
        let inner_radius = sight.radius;
        let current_alpha = smoothstep(inner_radius, outer_radius, distance);
        final_alpha = min(final_alpha, current_alpha);
    }
    
    return vec4f(settings.color.rgb, final_alpha * settings.color.a);
}