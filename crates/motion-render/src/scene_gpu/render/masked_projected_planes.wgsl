// Independently authored projective source mapping for calibrated photo geometry.
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
@group(0) @binding(2) var coverage: texture_2d<f32>;
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
    @location(1) uvq: vec3<f32>,
};
@vertex
fn vs(@location(0) clip: vec4<f32>, @location(1) color: vec4<f32>, @location(2) uvq: vec3<f32>) -> Output {
    var out: Output;
    out.position = clip;
    out.color = color;
    out.uvq = uvq;
    return out;
}
@fragment
fn fs(input: Output) -> @location(0) vec4<f32> {
    // Match perspective image planes' f32 bilinear weights. Hardware sampler
    // weight quantization otherwise changes dark gradients when subdividing
    // the same photo into triangles at different camera distances.
    let dimensions = vec2<i32>(textureDimensions(image));
    let uv = clamp(input.uvq.xy / input.uvq.z, vec2(0.0), vec2(1.0));
    let p = uv * vec2<f32>(dimensions) - vec2(0.5);
    let lower = vec2<i32>(floor(p));
    let fraction = fract(p);
    let lo = vec2(0);
    let hi = dimensions - vec2(1);
    let a = textureLoad(image, clamp(lower, lo, hi), 0);
    let b = textureLoad(image, clamp(lower + vec2(1, 0), lo, hi), 0);
    let c = textureLoad(image, clamp(lower + vec2(0, 1), lo, hi), 0);
    let d = textureLoad(image, clamp(lower + vec2(1, 1), lo, hi), 0);
    let mask_size = vec2<i32>(textureDimensions(coverage));
    let mask_p = uv * vec2<f32>(mask_size) - vec2(0.5);
    let mask_lo = vec2<i32>(floor(mask_p));
    let mask_f = fract(mask_p);
    let mask_hi = mask_size - vec2(1);
    let ma = textureLoad(coverage, clamp(mask_lo, lo, mask_hi), 0).r;
    let mb = textureLoad(coverage, clamp(mask_lo + vec2(1, 0), lo, mask_hi), 0).r;
    let mc = textureLoad(coverage, clamp(mask_lo + vec2(0, 1), lo, mask_hi), 0).r;
    let md = textureLoad(coverage, clamp(mask_lo + vec2(1, 1), lo, mask_hi), 0).r;
    let mask = clamp(mix(mix(ma, mb, mask_f.x), mix(mc, md, mask_f.x), mask_f.y), 0.0, 1.0);
    return mix(mix(a, b, fraction.x), mix(c, d, fraction.x), fraction.y) * input.color * mask;
}
