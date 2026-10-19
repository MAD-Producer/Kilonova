struct Weight { value: f32, _padding0: f32, _padding1: f32, _padding2: f32 }
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> weight: Weight;

@vertex fn vertex_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}

// The sample attachment is sRGB-encoded linear premultiplied RGB. Decode once
// on load, then sum RGB and alpha in a floating-point linear attachment.
@fragment fn accumulate(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(p.xy), 0) * weight.value;
}

// Encode to the renderer's sRGB output only after the complete exposure.
@fragment fn resolve(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(p.xy), 0);
}
