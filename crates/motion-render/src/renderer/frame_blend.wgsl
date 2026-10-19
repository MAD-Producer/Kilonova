struct Mix { weight: f32, _padding0: f32, _padding1: f32, _padding2: f32 }
@group(0) @binding(0) var left: texture_2d<f32>;
@group(0) @binding(1) var right: texture_2d<f32>;
@group(0) @binding(2) var<uniform> settings: Mix;

@vertex fn vertex_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}

// Inputs are sRGB views of linear-premultiplied pixels. Texture loads decode
// RGB; the sRGB attachment encodes it after weighting. Alpha stays linear.
@fragment fn fragment_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let xy = vec2<i32>(p.xy);
    return mix(textureLoad(left, xy, 0), textureLoad(right, xy, 0), settings.weight);
}
