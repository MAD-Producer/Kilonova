// Independently authored implementation of separable source-over blending.
// W3C Compositing and Blending Level 1, 2024-03-21, sections 6/10.
// Add/Subtract/Divide are bounded arithmetic transfer extensions.
fn ng_blend_channel(mode:u32,b:f32,s:f32)->f32 {
    switch mode {
        case 1u: {return min(b+s,1.0);}
        case 2u: {return b*s;}
        case 3u: {return b+s-b*s;}
        case 4u: {
            if b<=0.5 {return 2.0*b*s;}
            return 1.0-2.0*(1.0-b)*(1.0-s);
        }
        case 5u: {return min(b,s);}
        case 6u: {return max(b,s);}
        case 7u: {return abs(b-s);}
        case 8u: {return b+s-2.0*b*s;}
        case 9u: {return max(b-s,0.0);}
        case 10u: {
            if s==0.0 {return 1.0;}
            return min(b/s,1.0);
        }
        case 11u: {
            if b==0.0 {return 0.0;}
            if s==1.0 {return 1.0;}
            return min(b/(1.0-s),1.0);
        }
        case 12u: {
            if b==1.0 {return 1.0;}
            if s==0.0 {return 0.0;}
            return 1.0-min((1.0-b)/s,1.0);
        }
        case 13u: {
            if s<=0.5 {return 2.0*b*s;}
            return 1.0-2.0*(1.0-b)*(1.0-s);
        }
        case 14u: {
            if s<=0.5 {return b-(1.0-2.0*s)*b*(1.0-b);}
            var d=sqrt(b);
            if b<=0.25 {d=((16.0*b-12.0)*b+4.0)*b;}
            return b+(2.0*s-1.0)*(d-b);
        }
        default: {return s;}
    }
}
fn ng_blend(mode:u32,backdrop:vec4<f32>,source:vec4<f32>,factor:f32)->vec4<f32> {
    // Transfer operates on straight colors in the graph's working space.
    // Factor controls source opacity, not interpolation of unrelated alphas.
    let b=clamp(ng_to_straight(backdrop),vec4(0.0),vec4(1.0));
    let s=clamp(ng_to_straight(source),vec4(0.0),vec4(1.0));
    let sa=s.a*clamp(factor,0.0,1.0);
    let alpha=sa+b.a*(1.0-sa);
    if alpha<=0.0 {return vec4(0.0);}
    let transfer=vec3(ng_blend_channel(mode,b.r,s.r),ng_blend_channel(mode,b.g,s.g),ng_blend_channel(mode,b.b,s.b));
    let rgb=b.rgb*b.a*(1.0-sa)+s.rgb*sa*(1.0-b.a)+transfer*b.a*sa;
    return ng_from_straight(vec4(rgb/alpha,alpha));
}
