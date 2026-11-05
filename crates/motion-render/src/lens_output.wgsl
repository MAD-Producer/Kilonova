struct Parameters {
    inverse_x: vec4<f32>, inverse_y: vec4<f32>,
    ideal_x: vec4<f32>, ideal_y: vec4<f32>,
    radial: vec4<f32>, tangential: vec4<f32>,
    raster: vec4<f32>, output_scale: vec4<f32>,
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> p: Parameters;
@vertex fn vertex_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    let x=f32((i<<1u)&2u);let y=f32(i&2u);
    return vec4(x*2.-1.,1.-y*2.,0.,1.);
}
fn displacement(q:vec2<f32>)->vec2<f32> {
    let r2=dot(q,q);let r=r2*(p.radial.x+r2*(p.radial.y+r2*p.radial.z));
    let t=p.tangential.xy;
    return q*r+vec2(2.*t.x*q.x*q.y+t.y*(r2+2.*q.x*q.x),
        t.x*(r2+2.*q.y*q.y)+2.*t.y*q.x*q.y);
}
fn load_pixel(at:vec2<i32>)->vec4<f32> {
    let size=vec2<i32>(textureDimensions(source));
    if any(at<vec2(0)) || any(at>=size) {return vec4(0.);}
    return textureLoad(source,at,0);
}
@fragment fn fragment_main(@builtin(position) at:vec4<f32>)->@location(0) vec4<f32> {
    if p.tangential.z>0.5 {return textureLoad(source,vec2<i32>(at.xy),0);}
    let point=vec3(at.xy*p.output_scale.xy,1.);
    let d=vec2(dot(p.inverse_x.xyz,point),dot(p.inverse_y.xyz,point));
    var q=d;
    if p.radial.w>0. {
        for(var i=0u;i<48u;i+=1u) {
            let g=displacement(q);
            if p.tangential.w==1. {q=d-g;}
            else {q=q+p.tangential.w*(d-q-g);}
        }
    }
    let ideal=vec2(dot(p.ideal_x.xyz,vec3(q,1.)),dot(p.ideal_y.xyz,vec3(q,1.)));
    let uv=(ideal-p.raster.xy)/p.raster.zw;
    let pixel=uv*vec2<f32>(textureDimensions(source))-.5;
    let base=vec2<i32>(floor(pixel));let f=fract(pixel);
    // Manual bilinear reads support non-filterable Float32 storage. Values
    // remain premultiplied linear color, including negative/HDR channels.
    return mix(mix(load_pixel(base),load_pixel(base+vec2(1,0)),f.x),
        mix(load_pixel(base+vec2(0,1)),load_pixel(base+vec2(1,1)),f.x),f.y);
}
