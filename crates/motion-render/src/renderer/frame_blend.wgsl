struct Mix { weight: f32, _padding0: f32, _padding1: f32, _padding2: f32 }
@group(0) @binding(0) var left: texture_2d<f32>;
@group(0) @binding(1) var right: texture_2d<f32>;
@group(0) @binding(2) var<uniform> settings: Mix;

@vertex fn vertex_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}

// Match the frozen CPU reader's sRGB encoding and nearest-byte quantization.
// Letting the attachment encode a value near a byte boundary can choose a
// different byte on different backends. Inputs still decode through sRGB views
// and are mixed in linear premultiplied space; alpha never uses the transfer.
fn encoded_mix(p: vec4<f32>) -> vec4<f32> {
    let xy = vec2<i32>(p.xy);
    let c = mix(textureLoad(left, xy, 0), textureLoad(right, xy, 0), settings.weight);
    let rgb = select(1.055 * pow(max(c.rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055,
                     12.92 * c.rgb, c.rgb <= vec3<f32>(0.0031308));
    return floor(clamp(vec4<f32>(rgb, c.a), vec4<f32>(0.0), vec4<f32>(1.0)) * 255.0 + 0.5) / 255.0;
}
@fragment fn fragment_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return encoded_mix(p);
}
// Preserve support for backends without format-reinterpreting views, as in
// video_yuv.wgsl. Re-encode an already quantized byte, not the boundary value.
@fragment fn fragment_srgb(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let c = encoded_mix(p);
    let linear = select(pow((c.rgb + 0.055) / 1.055, vec3<f32>(2.4)),
                        c.rgb / 12.92, c.rgb <= vec3<f32>(0.04045));
    return vec4<f32>(linear, c.a);
}
