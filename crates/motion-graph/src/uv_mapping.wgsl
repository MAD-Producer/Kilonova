// Forward coordinate transform: scale about pivot, rotate in degrees, translate.
// UV's downward Y axis makes positive rotation clockwise on the displayed image.
fn ng_uv_mapping(coordinate: vec2<f32>, translation: vec2<f32>, rotation: f32,
    scale: vec2<f32>, pivot: vec2<f32>) -> vec2<f32> {
    let offset = (coordinate - pivot) * scale;
    let angle = rotation * 0.017453292519943295;
    let sine = sin(angle);
    let cosine = cos(angle);
    return vec2<f32>(cosine * offset.x - sine * offset.y,
        sine * offset.x + cosine * offset.y) + pivot + translation;
}
