@group(0) @binding(0) var field: texture_2d<f32>;
@group(0) @binding(1) var<uniform> viewport: vec4f;

@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let p = array<vec2f, 3>(vec2f(-1., -1.), vec2f(3., -1.), vec2f(-1., 3.));
    return vec4f(p[index], 0., 1.);
}
struct Seed {
    @location(0) color: vec4f,
    @builtin(frag_depth) depth: f32,
}
@fragment fn fs(@builtin(position) position: vec4f) -> Seed {
    let uv = (position.xy - viewport.xy) / viewport.zw;
    let size = textureDimensions(field);
    let pixel = clamp(vec2i(floor(uv * vec2f(size))), vec2i(0), vec2i(size) - vec2i(1));
    let sample = textureLoad(field, pixel, 0).rg;
    if sample.y == 0. { discard; }
    return Seed(vec4f(0.), sample.x);
}
