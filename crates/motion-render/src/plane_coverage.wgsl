// Square-pixel box integral of a locally linear half-plane. Unlike smoothstep,
// this preserves coverage when an edge becomes almost parallel to a pixel axis.
fn plane_half_coverage(distance:f32, gradient:vec2<f32>)->f32 {
    let a=max(abs(gradient.x),abs(gradient.y));
    let b=min(abs(gradient.x),abs(gradient.y));
    if a<1.0e-12 {return select(0.0,1.0,distance>=0.0);}
    if b<a*1.0e-5 {return clamp(distance/a+0.5,0.0,1.0);}
    let half_width=(a+b)*0.5;
    let d=abs(distance);
    if d>=half_width {return select(0.0,1.0,distance>=0.0);}
    if d<=(a-b)*0.5 {return 0.5+distance/a;}
    let tail=(half_width-d)*(half_width-d)/(2.0*a*b);
    return select(tail,1.0-tail,distance>=0.0);
}
fn plane_coverage(uv:vec2<f32>,scale:vec2<f32>)->f32 {
    if !PLANE_ANTIALIAS {return 1.0;}
    let p=uv/max(scale,vec2(1.0e-12));
    let dx=dpdx(p);let dy=dpdy(p);
    let u=vec2(dx.x,dy.x);let v=vec2(dx.y,dy.y);
    let x=plane_half_coverage(p.x,u)-plane_half_coverage(p.x-1.0,u);
    let y=plane_half_coverage(p.y,v)-plane_half_coverage(p.y-1.0,v);
    return clamp(x*y,0.0,1.0);
}
fn plane_sample_uv(uv:vec2<f32>,scale:vec2<f32>,size:vec2<u32>)->vec2<f32> {
    if !PLANE_ANTIALIAS {return uv;}
    // Pooled textures can exceed the used rectangle. The outside fringe must
    // extend the actual edge texel, never sample unused scratch-pool padding.
    let half_texel=0.5/vec2<f32>(size);
    return clamp(uv,half_texel,max(half_texel,scale-half_texel));
}
