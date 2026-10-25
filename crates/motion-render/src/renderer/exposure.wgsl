struct Weight { has_sample: f32, _padding0: f32, _padding1: f32, _padding2: f32 }
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> weight: Weight;

@vertex fn vertex_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}

// Samples are linear premultiplied values. An sRGB attachment decodes once on
// load; linear integer and floating attachments need no transfer conversion.
// Blend RGB and alpha with the CPU's current / cumulative weight.
// A missing phase still contributes its transparent shutter interval, without
// reading pixels left over from the previous phase.
@fragment fn accumulate(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    if weight.has_sample == 0.0 { return vec4<f32>(0.0); }
    return textureLoad(source, vec2<i32>(p.xy), 0);
}

// Only the UNORM8 mode uses nearest-byte sRGB output quantization. Other modes
// preserve the accumulated working values, including floating HDR/negative RGB.
@fragment fn resolve(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(source, vec2<i32>(p.xy), 0);
    if !OUTPUT_UNORM8 { return c; }
    let alpha = floor(clamp(c.a, 0.0, 1.0) * 255.0 + 0.5) / 255.0;
    // The output stores an 8-bit alpha. Reassociate RGB with that representable
    // alpha before encoding, so tiny coverage does not change the straight
    // color and zero-alpha children cannot retain invisible RGB energy.
    let rgb = clamp(c.rgb / max(c.a, 1e-20), vec3<f32>(0.0), vec3<f32>(1.0)) * alpha;
    // Quantize at the center of each output byte before the attachment's sRGB
    // encoder, matching the frozen output's nearest-byte contract. This keeps a
    // half-exposed pure color pure after straight-alpha readback on all backends.
    let encoded = select(1.055 * pow(rgb, vec3<f32>(1.0 / 2.4)) - 0.055,
                         12.92 * rgb, rgb <= vec3<f32>(0.0031308));
    let bytes = floor(clamp(vec4<f32>(encoded, alpha), vec4<f32>(0.0), vec4<f32>(1.0)) * 255.0 + 0.5) / 255.0;
    let linear = select(pow((bytes.rgb + 0.055) / 1.055, vec3<f32>(2.4)),
                        bytes.rgb / 12.92, bytes.rgb <= vec3<f32>(0.04045));
    return vec4<f32>(linear, bytes.a);
}
