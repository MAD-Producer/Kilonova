// Independently authored 2D lattice noise. No texture, permutation table or loop.
fn ng_noise_hash(cell: vec2<i32>, seed: u32) -> u32 {
    var h = (bitcast<u32>(cell.x) ^ seed) * 1000003u
          + (bitcast<u32>(cell.y) + 65537u) * 1000033u;
    h = (h ^ (h >> 16u)) * 1000037u;
    h = (h ^ (h >> 13u)) * 1000039u;
    return h ^ (h >> 16u);
}
fn ng_noise_corner(cell: vec2<i32>, delta: vec2<f32>, seed: u32, mode: u32) -> f32 {
    let h = ng_noise_hash(cell, seed);
    if mode == 0u {
        return f32(h >> 8u) / 16777215.0;
    }
    let diagonal = 0.7071067811865476;
    var gradient = vec2<f32>(1.0, 0.0);
    switch h & 7u {
        case 1u: { gradient = vec2<f32>(diagonal, diagonal); }
        case 2u: { gradient = vec2<f32>(0.0, 1.0); }
        case 3u: { gradient = vec2<f32>(-diagonal, diagonal); }
        case 4u: { gradient = vec2<f32>(-1.0, 0.0); }
        case 5u: { gradient = vec2<f32>(-diagonal, -diagonal); }
        case 6u: { gradient = vec2<f32>(0.0, -1.0); }
        case 7u: { gradient = vec2<f32>(diagonal, -diagonal); }
        default: {}
    }
    return dot(gradient, delta);
}
fn ng_noise(position: vec2<f32>, seed: u32, mode: u32) -> f32 {
    // Bound float-to-integer conversion for extreme animated coordinates.
    let p = clamp(position, vec2<f32>(-1000000.0), vec2<f32>(1000000.0));
    let cell = vec2<i32>(floor(p));
    let t = fract(p);
    let fade = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let a = ng_noise_corner(cell, t, seed, mode);
    let b = ng_noise_corner(cell + vec2<i32>(1, 0), t - vec2<f32>(1.0, 0.0), seed, mode);
    let c = ng_noise_corner(cell + vec2<i32>(0, 1), t - vec2<f32>(0.0, 1.0), seed, mode);
    let d = ng_noise_corner(cell + vec2<i32>(1, 1), t - vec2<f32>(1.0, 1.0), seed, mode);
    let n = mix(mix(a, b, fade.x), mix(c, d, fade.x), fade.y);
    return clamp(select(n, 0.5 + 0.5 * n, mode == 1u), 0.0, 1.0);
}
