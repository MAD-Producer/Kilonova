// Independently authored normalized-coordinate gradient; no textures or loops.
fn ng_gradient(uv: vec2<f32>, start: vec2<f32>, end: vec2<f32>, falloff: f32, mode: u32) -> f32 {
    let axis = end - start;
    let span2 = dot(axis, axis);
    if span2 <= 1.0e-12 {
        return 0.0;
    }
    let offset = uv - start;
    var factor = dot(offset, axis) / span2;
    if mode == 1u {
        factor = length(offset) / sqrt(span2);
    } else if mode == 2u {
        if dot(offset, offset) <= 1.0e-12 {
            return 0.0;
        }
        let angle = atan2(axis.x * offset.y - axis.y * offset.x, dot(axis, offset));
        factor = fract(angle / 6.283185307179586 + 1.0);
    }
    return pow(clamp(factor, 0.0, 1.0), clamp(falloff, 0.001, 1000.0));
}
