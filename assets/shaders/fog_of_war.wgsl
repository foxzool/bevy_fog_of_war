struct FogOfWarSettings {
    color: vec4f,
    screen_size: vec2f,
};
@group(0) @binding(0) var<uniform> settings: FogOfWarSettings;

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
    let center = vec2f(0.0, 0.0);
    let aspect_ratio = settings.screen_size.x / settings.screen_size.y;
    let corrected_position = vec2f(in.position.x * aspect_ratio, in.position.y);
    let distance = length(corrected_position - center);
    
    let inner_radius = 0.3;
    let outer_radius = 0.5;
    
    let alpha = smoothstep(inner_radius, outer_radius, distance) * settings.color.a;

    return vec4f(settings.color.rgb, alpha);
}