// Panel chrome: solid fills and atlas-sampled glyphs.
//
// The viewport transform arrives as a uniform rather than push constants so the
// pipeline needs no optional device feature.

struct SolidVertex {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) geometry: vec4<f32>,
    @location(3) params: vec4<f32>,
    @location(4) extra: vec4<f32>,
    @location(5) secondary: vec4<f32>,
}

struct GlyphVertex {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> transform: vec4<f32>;

@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

fn to_clip(position: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(position.x * transform.x - 1.0, position.y * transform.w + 1.0, 0.0, 1.0);
}

struct SolidOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) logical: vec2<f32>,
    @location(2) @interpolate(flat) geometry: vec4<f32>,
    @location(3) @interpolate(flat) params: vec4<f32>,
    @location(4) @interpolate(flat) extra: vec4<f32>,
    @location(5) @interpolate(flat) secondary: vec4<f32>,
}

struct GlyphOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn solid_vs(vertex: SolidVertex) -> SolidOutput {
    return SolidOutput(to_clip(vertex.position),vertex.color,vertex.position,vertex.geometry,vertex.params,vertex.extra,vertex.secondary);
}

fn rectangle_distance(point:vec2<f32>,half:vec2<f32>,radius:f32)->f32 {
    let q=abs(point)-max(half-vec2(radius),vec2(0.0));
    return length(max(q,vec2(0.0)))+min(max(q.x,q.y),0.0)-radius;
}
fn grid_distance(point:vec2<f32>,origin:vec2<f32>,step:f32)->f32 {
    let s=max(step,1.0);
    let d=abs(fract((point-origin)/s+vec2(0.5))-vec2(0.5))*s;
    return max(d.x,d.y)-0.5;
}
@fragment
fn solid_fs(v:SolidOutput)->@location(0) vec4<f32> {
    // Derivatives are computed uniformly, including helper invocations.
    let point=v.logical-(v.geometry.xy+v.geometry.zw*0.5);
    let radius=max(v.params.z,0.0);
    let outer=rectangle_distance(point,v.geometry.zw*0.5,radius);
    let inner=rectangle_distance(point,max(v.geometry.zw*0.5-vec2(v.params.y),vec2(0.0)),max(radius-v.params.y,0.0));
    let aa=max(fwidth(outer),0.001);
    let ia=max(fwidth(inner),0.001);
    let minor_distance=grid_distance(v.logical,v.extra.xy,v.extra.z);
    let major_distance=grid_distance(v.logical,v.extra.xy,v.extra.w);
    let ma=max(fwidth(minor_distance),0.001);
    let ga=max(fwidth(major_distance),0.001);
    if v.params.x<0.5 {return v.color;}
    if v.params.x<1.5 {
        let oc=1.0-smoothstep(-0.5*aa,0.5*aa,outer);
        var ic=oc;
        if v.params.y>0.0 {ic=1.0-smoothstep(-0.5*ia,0.5*ia,inner);}
        let bc=max(oc-ic,0.0);
        let alpha=v.color.a*ic+v.secondary.a*bc;
        let rgb=(v.color.rgb*v.color.a*ic+v.secondary.rgb*v.secondary.a*bc)/max(alpha,0.000001);
        return vec4(rgb,alpha);
    }
    if v.params.x<2.5 {
        let cell=floor((v.logical-v.geometry.xy)/max(v.params.y,1.0));
        return select(v.color,v.secondary,i32(cell.x+cell.y)%2!=0);
    }
    if v.params.x>3.5 {
        let p=v.logical-(v.extra.xy+v.extra.zw*0.5);
        let d=rectangle_distance(p,v.extra.zw*0.5,v.params.z);
        let coverage=1.0-smoothstep(-max(v.params.y,0.001)*0.5,max(v.params.y,0.001),d);
        return vec4(v.color.rgb,v.color.a*coverage);
    }
    let minor_coverage=(1.0-smoothstep(-0.5*ma,0.5*ma,minor_distance))*smoothstep(8.0,16.0,v.extra.z);
    let major_coverage=1.0-smoothstep(-0.5*ga,0.5*ga,major_distance);
    let alpha=max(v.color.a*minor_coverage,v.secondary.a*major_coverage);
    return vec4(select(v.color.rgb,v.secondary.rgb,major_coverage>minor_coverage),alpha);
}

@vertex
fn glyph_vs(vertex: GlyphVertex) -> GlyphOutput {
    return GlyphOutput(to_clip(vertex.position), vertex.uv, vertex.color);
}

@fragment
fn glyph_fs(vertex: GlyphOutput) -> @location(0) vec4<f32> {
    // The atlas stores white coverage; tint with the vertex colour.
    let coverage = textureSample(atlas, atlas_sampler, vertex.uv).a;
    return vec4<f32>(vertex.color.rgb, vertex.color.a * coverage);
}
