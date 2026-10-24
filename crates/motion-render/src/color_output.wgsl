struct Transform { flags: vec4<u32>, gain: f32, encode_srgb: u32, reserved: vec2<u32> }
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> transform: Transform;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vertex_main(@builtin(vertex_index) id: u32) -> Vertex {
    let positions = array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    let p=positions[id]; var result:Vertex;
    result.position=vec4(p,0.0,1.0); result.uv=vec2(p.x*0.5+0.5,0.5-p.y*0.5); return result;
}
fn load(p:vec2<i32>,last:vec2<i32>) -> vec4<f32> {
    let value=textureLoad(source,clamp(p,vec2<i32>(0),last),0);
    if value.a<=0.0 { return vec4(0.0); }
    var color=value.rgb;
    if transform.flags.y!=0u { color/=value.a; }
    if transform.flags.x!=0u { color=decode_srgb(color); }
    // Interpolate linear premultiplied values, including straight-alpha input.
    return vec4(color*value.a,value.a);
}
fn bilinear(uv:vec2<f32>) -> vec4<f32> {
    let size=textureDimensions(source); let p=uv*vec2<f32>(size)-0.5;
    let base=vec2<i32>(floor(p)); let fraction=fract(p); let last=vec2<i32>(size)-1;
    return mix(mix(load(base,last),load(base+vec2(1,0),last),fraction.x),
        mix(load(base+vec2(0,1),last),load(base+vec2(1,1),last),fraction.x),fraction.y);
}
fn decode_srgb(x:vec3<f32>) -> vec3<f32> {
    return select(pow(max((x+0.055)/1.055,vec3(0.0)),vec3(2.4)),x/12.92,x<=vec3(0.04045));
}
fn encode_srgb(x:vec3<f32>) -> vec3<f32> {
    return select(1.055*pow(max(x,vec3(0.0)),vec3(1.0/2.4))-0.055,x*12.92,x<=vec3(0.0031308));
}
@fragment fn fragment_main(input:Vertex) -> @location(0) vec4<f32> {
    let sampled=bilinear(input.uv); let alpha=clamp(sampled.a,0.0,1.0);
    // Zero source alpha produces transparent black. Division uses original
    // positive alpha before output clamping, in linear premultiplied space.
    if sampled.a<=0.0 { return vec4(0.0); }
    var color=sampled.rgb/sampled.a;
    color=max(color*transform.gain,vec3(0.0));
    if transform.flags.w!=0u { color=vec3(1.0)-vec3(1.0)/(vec3(1.0)+color); }
    else { color=min(color,vec3(1.0)); }
    if transform.flags.z!=0u { color*=alpha; }
    if transform.encode_srgb!=0u { color=encode_srgb(color); }
    return vec4(color,alpha);
}
