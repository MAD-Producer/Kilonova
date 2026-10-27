@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};
@vertex
fn vs(@location(0) clip: vec4<f32>, @location(1) color: vec4<f32>, @location(2) uv: vec2<f32>) -> Output {
    var out: Output;
    out.position = clip;
    out.color = color;
    out.uv = uv;
    return out;
}
@fragment
fn fs(input: Output) -> @location(0) vec4<f32> {
    return textureSample(image, image_sampler, input.uv) * input.color;
}
