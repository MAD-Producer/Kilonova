@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
struct Presentation { mode: vec4<u32>, uv: vec4<f32> };
@group(0) @binding(2) var<uniform> presentation: Presentation;
struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex fn vertex_main(@builtin(vertex_index) index: u32) -> Out {
    let p=vec2(f32((index<<1u)&2u)*2.0-1.0,f32(index&2u)*2.0-1.0);
    var out:Out;
    out.position=vec4(p,0.0,1.0);
    out.uv=vec2(p.x*0.5+0.5,0.5-p.y*0.5);
    return out;
}
fn encode(v:vec3<f32>)->vec3<f32> {
    return select(1.055*pow(max(v,vec3(0.0)),vec3(1.0/2.4))-0.055,
        12.92*v,v<=vec3(0.0031308));
}
fn decode(v:vec3<f32>)->vec3<f32> {
    return select(pow((v+0.055)/1.055,vec3(2.4)),v/12.92,v<=vec3(0.04045));
}
@fragment fn fragment_main(input:Out)->@location(0) vec4<f32> {
    // Canvas navigation uses pixel centers relative to this viewport. Interpolated
    // triangle UVs can round differently after an integer viewport translation,
    // changing filtered texels in another cell despite identical local geometry.
    // The ordinary full-surface path keeps its original interpolated UVs.
    let local=input.position.xy-bitcast<vec2<f32>>(presentation.mode.zw);
    let uv=select(input.uv,(local+presentation.uv.xy)*presentation.uv.zw,
        (presentation.mode.x&2u)!=0u);
    let c=textureSample(source,source_sampler,uv);
    if any(uv<vec2(0.0)) || any(uv>vec2(1.0)) { discard; }
    if presentation.mode.y>0u {
        let intensity=vec3(clamp(c[presentation.mode.y-1u],0.0,1.0));
        // sRGB surfaces encode shader RGB on write. Decode an already selected
        // display intensity so all four surface formats show the same raw value.
        if (presentation.mode.x&1u)!=0u { return vec4(decode(intensity),1.0); }
        return vec4(intensity,1.0);
    }
    if (presentation.mode.x&1u)!=0u { return c; }
    return vec4(encode(c.rgb),c.a);
}
