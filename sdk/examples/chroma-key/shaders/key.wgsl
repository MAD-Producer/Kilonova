// Independently authored demonstration kernel, under the repository MIT license.
// Classification uses nonnegative unit-sum chroma. Output RGB is never clamped.
fn positive_chroma(rgb: vec3<f32>) -> vec3<f32> {
    let positive = max(rgb, vec3(0.));
    let peak = max(positive.r, max(positive.g, positive.b));
    if peak <= 0. { return vec3(0.); }
    // Scale before summing so large finite Float32 highlights cannot overflow.
    let scaled = positive / peak;
    return scaled / (scaled.r + scaled.g + scaled.b);
}

fn main_fx(p: vec2<f32>) -> vec4<f32> {
    let pixel = sample_input(p);
    if pixel.a <= 0. { return vec4(0.); }
    let key = positive_chroma(fx.params[0].rgb);
    // Black supplies no chroma direction: explicitly keep the input unchanged.
    if max(key.r, max(key.g, key.b)) <= 0. { return pixel; }
    let positive = max(pixel.rgb, vec3(0.));
    var coverage = 1.;
    if max(positive.r, max(positive.g, positive.b)) > 0. {
        let distance = length(positive_chroma(pixel.rgb) - key);
        let tolerance = fx.params[1].x;
        let softness = fx.params[2].x;
        if softness <= 0. {
            coverage = select(0., 1., distance > tolerance);
        } else {
            coverage = smoothstep(tolerance, tolerance + softness, distance);
        }
    }
    if coverage <= 0. { return vec4(0.); }
    var rgb = pixel.rgb;
    let spill = fx.params[3].x;
    if spill > 0. {
        // Suppress only the positive excess of the key's dominant channel.
        // A zero floor on the comparison avoids overflow against negative HDR.
        if key.g >= key.r && key.g >= key.b {
            rgb.g -= spill * max(0., rgb.g - max(0., max(rgb.r, rgb.b)));
        } else if key.b >= key.r {
            rgb.b -= spill * max(0., rgb.b - max(0., max(rgb.r, rgb.g)));
        } else {
            rgb.r -= spill * max(0., rgb.r - max(0., max(rgb.g, rgb.b)));
        }
    }
    return vec4(rgb, pixel.a * coverage);
}
