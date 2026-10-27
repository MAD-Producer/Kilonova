struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
};
@vertex
fn vs(@location(0) clip: vec4<f32>, @location(1) color: vec4<f32>) -> Output {
    var out: Output;
    out.position = clip;
    out.color = color;
    return out;
}
@fragment
fn fs(input: Output) -> @location(0) vec4<f32> {
    return input.color;
}
