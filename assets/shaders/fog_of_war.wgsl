struct FogOfWarSettings {
    color: vec4f,
    screen_size: vec2f,
};

struct FogSight2D {
    position: vec2f,
    inner_radius: f32,
    outer_radius: f32,
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
    let aspect_ratio = settings.screen_size.x / settings.screen_size.y;
    let screen_position = vec2f(in.position.x * aspect_ratio, in.position.y);
    
    var final_alpha = 1.0;
    
    for(var i = 0u; i < arrayLength(&sight_points); i++) {
        let sight = sight_points[i];
        let sight_pos = vec2f(sight.position.x * aspect_ratio, sight.position.y);
        let distance = length(screen_position - sight_pos);
        let alpha = smoothstep(sight.inner_radius * aspect_ratio, sight.outer_radius * aspect_ratio, distance);
        final_alpha = min(final_alpha, alpha);
    }
    
    return vec4f(settings.color.rgb, final_alpha * settings.color.a);
}