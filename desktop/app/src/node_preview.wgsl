struct Mode { encoded: u32, premultiplied: u32, surface_srgb: u32, padding: u32 };
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
@group(0) @binding(2) var<uniform> mode: Mode;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vertex_main(@builtin(vertex_index) index: u32) -> Vertex {
    let p = vec2(f32((index << 1u) & 2u)*2.0-1.0, f32(index & 2u)*2.0-1.0);
    var out: Vertex; out.position = vec4(p,0.0,1.0); out.uv = vec2(p.x*0.5+0.5,0.5-p.y*0.5); return out;
}
fn decode(v:vec3<f32>)->vec3<f32> {return select(pow((max(v,vec3(0.0))+0.055)/1.055,vec3(2.4)),v/12.92,v<=vec3(0.04045));}
fn encode(v:vec3<f32>)->vec3<f32> {return select(1.055*pow(max(v,vec3(0.0)),vec3(1.0/2.4))-0.055,12.92*v,v<=vec3(0.0031308));}
@fragment fn fragment_main(input:Vertex)->@location(0) vec4<f32> {
    let c=textureSample(image,image_sampler,input.uv); let alpha=clamp(c.a,0.0,1.0); var rgb=c.rgb;
    if mode.premultiplied!=0u {rgb=rgb/max(alpha,0.000001);}
    if mode.encoded!=0u {rgb=decode(rgb);}
    let tile=vec2<u32>(floor(input.uv*16.0));let checker=select(vec3(0.038),vec3(0.065),(tile.x+tile.y)%2u==0u);
    var visible=rgb*alpha+checker*(1.0-alpha);
    if mode.surface_srgb==0u {visible=encode(visible);}
    return vec4(visible,1.0);
}
