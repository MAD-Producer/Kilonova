// Square-pixel box integral of a locally linear half-plane. Unlike smoothstep,
// this preserves coverage when an edge becomes almost parallel to a pixel axis.
fn plane_half_coverage(distance:f32, gradient:vec2<f32>)->f32 {
    let a=max(abs(gradient.x),abs(gradient.y));
    let b=min(abs(gradient.x),abs(gradient.y));
    // Keep derivative-dependent coverage straight-line. In particular, guard
    // every denominator even when its result is discarded by select: shader
    // backends may evaluate both sides for helper lanes at a primitive edge.
    let safe_a=max(a,1.0e-12);
    let safe_b=max(b,safe_a*1.0e-5);
    let linear=clamp(distance/safe_a+0.5,0.0,1.0);
    let half_width=(a+b)*0.5;
    let d=abs(distance);
    let edge=clamp(half_width-d,0.0,b);
    let tail=(edge/safe_a)*(edge/safe_b)*0.5;
    let signed_tail=select(tail,1.0-tail,distance>=0.0);
    let integrated=select(signed_tail,linear,d<=(a-b)*0.5);
    let regular=select(integrated,linear,b<a*1.0e-5);
    return select(regular,select(0.0,1.0,distance>=0.0),a<1.0e-12);
}
fn plane_coverage(uv:vec2<f32>,scale:vec2<f32>,uv_dx:vec2<f32>,uv_dy:vec2<f32>)->f32 {
    if !PLANE_ANTIALIAS {return 1.0;}
    let safe_scale=max(scale,vec2(1.0e-12));
    let p=uv/safe_scale;
    // Differentiate the incoming interpolator at the fragment entry point.
    // Normalize afterward: scale is uniform, and this avoids derivatives of
    // helper-function temporaries on partial quads at an odd viewport edge.
    let dx=uv_dx/safe_scale;let dy=uv_dy/safe_scale;
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
