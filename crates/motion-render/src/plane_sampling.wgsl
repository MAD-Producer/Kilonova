// BSP cuts introduce new rasterization vertices. Hardware filter-weight
// quantization can amplify their small UV differences near dark checker edges.
// Perspective planes keep the same clamp-to-edge, linear-premultiplied filter,
// but evaluate its four weights in f32. Flat composition draws retain the
// ordinary sampler. No additional textures, uniforms or readback are required.
fn sample_plane_texture(source:texture_2d<f32>, linear_sampler:sampler, uv:vec2<f32>)->vec4<f32> {
    if abs(draw.mvp[0].w)+abs(draw.mvp[1].w)+abs(draw.mvp[2].w)==0.0 {
        return textureSample(source,linear_sampler,uv);
    }
    let dimensions=vec2<i32>(textureDimensions(source));
    if all(dimensions==vec2(1)) {return textureLoad(source,vec2(0),0);}
    let p=clamp(uv,vec2(0.0),vec2(1.0))*vec2<f32>(dimensions)-vec2(0.5);
    let lower=vec2<i32>(floor(p));let fraction=fract(p);
    let hi=dimensions-vec2(1);let lo=vec2(0);
    let a=textureLoad(source,clamp(lower,lo,hi),0);
    let b=textureLoad(source,clamp(lower+vec2(1,0),lo,hi),0);
    let c=textureLoad(source,clamp(lower+vec2(0,1),lo,hi),0);
    let d=textureLoad(source,clamp(lower+vec2(1,1),lo,hi),0);
    return mix(mix(a,b,fraction.x),mix(c,d,fraction.x),fraction.y);
}
