// Independently authored, bounded 49-tap disk gather. All colors stay in the
// native working texture; there is no SDR encode, clamp or alpha conversion.
struct Camera {
    inverse: mat4x4f,
    eye: vec4f,
    forward: vec4f,
    viewport: vec4f,
    aperture: vec4f, // focus distance, diameter, maximum pixel radius, zoom pixels
}
@group(0) @binding(0) var color: texture_2d<f32>;
// DEPTH_DECLARATION
@group(0) @binding(2) var<uniform> camera: Camera;

@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let p = array<vec2f, 3>(vec2f(-1., -1.), vec2f(3., -1.), vec2f(-1., 3.));
    return vec4f(p[index], 0., 1.);
}
fn axial_distance(pixel: vec2i) -> f32 {
    // DEPTH_LOAD
    if depth >= 1. { return 1.e20; }
    let uv = (vec2f(pixel) + vec2f(0.5) - camera.viewport.xy) / camera.viewport.zw;
    let h = camera.inverse * vec4f(uv.x * 2. - 1., 1. - uv.y * 2., depth, 1.);
    if abs(h.w) < 1.e-12 { return 1.e20; }
    return max(dot(h.xyz / h.w - camera.eye.xyz, camera.forward.xyz), 1.e-6);
}
fn radius(distance: f32) -> f32 {
    return min(camera.aperture.z, camera.aperture.y * 0.5 * camera.aperture.w
        * abs(1. / camera.aperture.x - 1. / distance));
}
@fragment fn fs(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let pixel = vec2i(position.xy);
    let center = textureLoad(color, pixel, 0);
    if any(position.xy < camera.viewport.xy)
        || any(position.xy >= camera.viewport.xy + camera.viewport.zw) { return center; }
    let distance = axial_distance(pixel);
    let blur = radius(distance);
    if blur < 0.5 { return center; }
    let lo = vec2i(ceil(camera.viewport.xy - vec2f(0.5)));
    let hi = min(vec2i(textureDimensions(color)) - vec2i(1),
        vec2i(ceil(camera.viewport.xy + camera.viewport.zw - vec2f(0.5))) - vec2i(1));
    var sum = center;
    var weight = 1.;
    for (var ring = 1; ring <= 3; ring++) {
        let count = ring * 8;
        for (var i = 0; i < count; i++) {
            let angle = 6.28318530718 * f32(i) / f32(count);
            let offset = vec2f(cos(angle), sin(angle)) * blur * f32(ring) / 3.;
            let q = clamp(vec2i(floor(position.xy + offset)), lo, hi);
            // A defocused background must not pull a sharply focused foreground
            // into its disk. This approximation cannot expose hidden surfaces.
            let neighbor_distance = axial_distance(q);
            if neighbor_distance < distance && radius(neighbor_distance) < 0.5 { continue; }
            sum += textureLoad(color, q, 0);
            weight += 1.;
        }
    }
    return sum / weight;
}
