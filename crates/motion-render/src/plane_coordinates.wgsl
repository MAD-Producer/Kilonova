// Evaluate one layer-wide homography at the physical pixel center. BSP cuts
// cannot change its UV or edge derivatives through a different triangle's
// interpolation rounding (particularly visible when unpremultiplying tiny alpha).
struct PlaneCoordinates { uv:vec2<f32>, dx:vec2<f32>, dy:vec2<f32> };
fn plane_pixel_grid()->vec2<f32> {return vec2(draw.screen_uv[0].w,draw.screen_uv[1].w);}
fn plane_coordinates(p:vec4<f32>,uv:vec2<f32>,dx:vec2<f32>,dy:vec2<f32>)->PlaneCoordinates {
    if draw.screen_uv[2].w==0.0 {return PlaneCoordinates(uv,dx,dy);}
    let point=vec3(p.xy,1.0);
    let numerator=vec2(dot(draw.screen_uv[0].xyz,point),dot(draw.screen_uv[1].xyz,point));
    let denominator=dot(draw.screen_uv[2].xyz,point);
    let value=numerator/denominator;
    let derivative_x=(vec2(draw.screen_uv[0].x,draw.screen_uv[1].x)-value*draw.screen_uv[2].x)/denominator;
    let derivative_y=(vec2(draw.screen_uv[0].y,draw.screen_uv[1].y)-value*draw.screen_uv[2].y)/denominator;
    return PlaneCoordinates(value*draw.uv_scale.xy,derivative_x*draw.uv_scale.xy,derivative_y*draw.uv_scale.xy);
}
