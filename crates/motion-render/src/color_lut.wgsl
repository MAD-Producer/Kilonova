struct Settings {
    lookup: vec4<u32>,
    domain_min: vec4<f32>,
    domain_max: vec4<f32>,
    output: vec4<u32>,
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> settings: Settings;
@group(0) @binding(2) var<storage, read> samples: array<vec4<f32>>;

@vertex fn vertex_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
    return vec4(p[i], 0., 1.);
}

fn fetch(p: vec3<u32>) -> vec3<f32> {
    let size = settings.lookup.x;
    return samples[p.x + size * (p.y + size * p.z)].rgb;
}

fn transform(rgb: vec3<f32>) -> vec3<f32> {
    let maximum = settings.lookup.x - 1u;
    let coordinate = (clamp(rgb, settings.domain_min.rgb, settings.domain_max.rgb) - settings.domain_min.rgb)
        / (settings.domain_max.rgb - settings.domain_min.rgb) * f32(maximum);
    let lo = vec3<u32>(floor(coordinate));
    let hi = min(lo + vec3(1u), vec3(maximum));
    let fraction = coordinate - vec3<f32>(lo);
    if settings.lookup.y == 0u {
        if settings.lookup.z == 0u {
            let nearest = vec3<u32>(floor(coordinate + vec3(0.5)));
            return vec3(samples[nearest.x].r, samples[nearest.y].g, samples[nearest.z].b);
        }
        let a = vec3(samples[lo.x].r, samples[lo.y].g, samples[lo.z].b);
        let b = vec3(samples[hi.x].r, samples[hi.y].g, samples[hi.z].b);
        return a + fraction * (b - a);
    }
    if settings.lookup.z == 0u {
        return fetch(vec3<u32>(floor(coordinate + vec3(0.5))));
    }
    if settings.lookup.z == 2u {
        // Six tetrahedra, with XYZ tie order. Explicit vertices avoid dynamic
        // array/component l-values rejected by the Windows FXC backend.
        var first: vec3<u32>;
        var second: vec3<u32>;
        var weights: vec3<f32>;
        if fraction.x >= fraction.y {
            if fraction.y >= fraction.z {
                first = vec3(hi.x, lo.y, lo.z);
                second = vec3(hi.x, hi.y, lo.z);
                weights = fraction.xyz;
            } else {
                if fraction.x >= fraction.z {
                    first = vec3(hi.x, lo.y, lo.z);
                    second = vec3(hi.x, lo.y, hi.z);
                    weights = fraction.xzy;
                } else {
                    first = vec3(lo.x, lo.y, hi.z);
                    second = vec3(hi.x, lo.y, hi.z);
                    weights = fraction.zxy;
                }
            }
        } else {
            if fraction.x >= fraction.z {
                first = vec3(lo.x, hi.y, lo.z);
                second = vec3(hi.x, hi.y, lo.z);
                weights = fraction.yxz;
            } else {
                if fraction.y >= fraction.z {
                    first = vec3(lo.x, hi.y, lo.z);
                    second = vec3(lo.x, hi.y, hi.z);
                    weights = fraction.yzx;
                } else {
                    first = vec3(lo.x, lo.y, hi.z);
                    second = vec3(lo.x, hi.y, hi.z);
                    weights = fraction.zyx;
                }
            }
        }
        let a = fetch(lo);
        let b = fetch(first);
        let c = fetch(second);
        let d = fetch(hi);
        return a + weights.x * (b - a) + weights.y * (c - b) + weights.z * (d - c);
    }
    let x00 = mix(fetch(vec3(lo.x, lo.y, lo.z)), fetch(vec3(hi.x, lo.y, lo.z)), fraction.x);
    let x10 = mix(fetch(vec3(lo.x, hi.y, lo.z)), fetch(vec3(hi.x, hi.y, lo.z)), fraction.x);
    let x01 = mix(fetch(vec3(lo.x, lo.y, hi.z)), fetch(vec3(hi.x, lo.y, hi.z)), fraction.x);
    let x11 = mix(fetch(vec3(lo.x, hi.y, hi.z)), fetch(vec3(hi.x, hi.y, hi.z)), fraction.x);
    return mix(mix(x00, x10, fraction.y), mix(x01, x11, fraction.y), fraction.z);
}

@fragment fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = textureLoad(source, vec2<i32>(position.xy), 0);
    var rgb = pixel.rgb;
    if settings.lookup.w != 0u {
        if pixel.a == 0. { rgb = vec3(0.); }
        else { rgb /= pixel.a; }
    }
    var result = transform(rgb);
    if settings.output.x != 0u { result *= pixel.a; }
    return vec4(result, pixel.a);
}
