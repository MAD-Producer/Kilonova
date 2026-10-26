// Independently authored static material shader. Linear light, straight texture
// inputs and a premultiplied output. No implicit tone mapping or exposure.
struct LightData { position_kind:vec4<f32>, color_intensity:vec4<f32>, parameters:vec4<f32> }
struct DrawData {
    view_projection:mat4x4<f32>, world:mat4x4<f32>, normal_world:mat4x4<f32>,
    eye_scale:vec4<f32>, ambient:vec4<f32>, flags:vec4<u32>, lights:array<LightData,4>,
}
struct MapData { transform:vec4<f32>, options:vec4<u32> }
struct MaterialData {
    base:vec4<f32>, emissive_metallic:vec4<f32>, parameters:vec4<f32>,
    specular_exponent:vec4<f32>, ambient:vec4<f32>, flags:vec4<u32>, maps:array<MapData,7>,
}
@group(0) @binding(0) var<uniform> draw:DrawData;
@group(1) @binding(0) var<uniform> material:MaterialData;
@group(1) @binding(1) var base_map:texture_2d<f32>;
@group(1) @binding(2) var base_sampler:sampler;
@group(1) @binding(3) var detail_map:texture_2d<f32>;
@group(1) @binding(4) var detail_sampler:sampler;
@group(1) @binding(5) var normal_map:texture_2d<f32>;
@group(1) @binding(6) var normal_sampler:sampler;
@group(1) @binding(7) var ambient_map:texture_2d<f32>;
@group(1) @binding(8) var ambient_sampler:sampler;
@group(1) @binding(9) var emissive_map:texture_2d<f32>;
@group(1) @binding(10) var emissive_sampler:sampler;
@group(1) @binding(11) var opacity_map:texture_2d<f32>;
@group(1) @binding(12) var opacity_sampler:sampler;
@group(1) @binding(13) var exponent_map:texture_2d<f32>;
@group(1) @binding(14) var exponent_sampler:sampler;

struct VertexIn {
    @location(0) position:vec3<f32>, @location(1) normal:vec3<f32>,
    @location(2) tangent:vec4<f32>, @location(3) uv0:vec2<f32>,
    @location(4) uv1:vec2<f32>, @location(5) color:vec4<f32>,
}
struct VertexOut {
    @builtin(position) clip:vec4<f32>, @location(0) world:vec3<f32>,
    @location(1) normal:vec3<f32>, @location(2) tangent:vec4<f32>,
    @location(3) uv0:vec2<f32>, @location(4) uv1:vec2<f32>, @location(5) color:vec4<f32>,
}
@vertex fn vs(v:VertexIn)->VertexOut {
    var out:VertexOut;let world=draw.world*vec4<f32>(v.position,1.);
    out.clip=draw.view_projection*world;out.world=world.xyz;
    out.normal=(draw.normal_world*vec4<f32>(v.normal,0.)).xyz;
    out.tangent=vec4<f32>((draw.world*vec4<f32>(v.tangent.xyz,0.)).xyz,v.tangent.w*select(1.,-1.,draw.flags.w!=0u));
    out.uv0=v.uv0;out.uv1=v.uv1;out.color=v.color;return out;
}
fn unit(v:vec3<f32>,fallback:vec3<f32>)->vec3<f32> {
    let cost=dot(v,v);if cost>1e-20 {return v*inverseSqrt(cost);}return fallback;
}
fn uv(i:u32,v:VertexOut)->vec2<f32> {
    let value=select(v.uv0,v.uv1,material.maps[i].options.x==1u);
    return value*material.maps[i].transform.zw+material.maps[i].transform.xy;
}
fn scalar(v:vec4<f32>,channel:u32)->f32 {
    switch channel {case 2u:{return v.g;}case 3u:{return v.b;}case 4u:{return v.a;}case 5u:{return dot(v.rgb,vec3<f32>(.2126,.7152,.0722));}default:{return v.r;}}
}
fn rgb(v:vec4<f32>,channel:u32)->vec3<f32> {
    if channel==0u{return v.rgb;}return vec3<f32>(scalar(v,channel));
}
fn phong_power(value:f32,exponent:f32)->f32 {
    if exponent==0.{return 1.;}if value<=0.{return 0.;}return pow(value,exponent);
}
fn brdf(base:vec3<f32>,metallic:f32,roughness:f32,n:vec3<f32>,v:vec3<f32>,l:vec3<f32>)->vec3<f32> {
    let no_l=max(dot(n,l),0.);let no_v=max(dot(n,v),1e-5);
    let half_vector=unit(v+l,n);let no_h=max(dot(n,half_vector),0.);let vo_h=max(dot(v,half_vector),0.);
    // A delta lobe at zero roughness cannot be represented by finite raster samples.
    let alpha=max(roughness*roughness,1e-4);let alpha2=alpha*alpha;
    let denominator=no_h*no_h*(alpha2-1.)+1.;
    let distribution=alpha2/(3.141592653589793*max(denominator*denominator,1e-12));
    let lambda_v=no_l*sqrt(no_v*no_v*(1.-alpha2)+alpha2);
    let lambda_l=no_v*sqrt(no_l*no_l*(1.-alpha2)+alpha2);
    let visibility=.5/max(lambda_v+lambda_l,1e-8);
    let f0=mix(vec3<f32>(.04),base,metallic);
    let fresnel=f0+(vec3<f32>(1.)-f0)*pow(1.-vo_h,5.);
    return ((vec3<f32>(1.)-fresnel)*(1.-metallic)*base/3.141592653589793+distribution*visibility*fresnel)*no_l;
}

@fragment fn fs(v:VertexOut,@builtin(front_facing) front:bool)->@location(0) vec4<f32> {
    // All sampling/derivatives precede alpha discard and varying control flow.
    let base_tex=textureSample(base_map,base_sampler,uv(0u,v));
    let detail_tex=textureSample(detail_map,detail_sampler,uv(1u,v));
    let normal_tex=textureSample(normal_map,normal_sampler,uv(2u,v));
    let ambient_tex=textureSample(ambient_map,ambient_sampler,uv(3u,v));
    let emissive_tex=textureSample(emissive_map,emissive_sampler,uv(4u,v));
    let opacity_tex=textureSample(opacity_map,opacity_sampler,uv(5u,v));
    let exponent_tex=textureSample(exponent_map,exponent_sampler,uv(6u,v));
    let dx=dpdx(v.world);let dy=dpdy(v.world);let coord=uv(2u,v);
    let uv_dx=dpdx(coord);let uv_dy=dpdy(coord);
    let height=scalar(normal_tex,material.maps[2].options.y);
    let height_dx=dpdx(height);let height_dy=dpdy(height);
    let face_sign=select(-1.,1.,front);
    var n=unit(v.normal,vec3<f32>(0.,0.,1.));
    if draw.flags.x==0u{n=unit(cross(dy,dx),vec3<f32>(0.,0.,1.))*face_sign;}
    if material.flags.z==1u {
        var tangent=vec3<f32>(0.);var bitangent=vec3<f32>(0.);
        if draw.flags.y!=0u {
            tangent=unit(v.tangent.xyz-n*dot(n,v.tangent.xyz),vec3<f32>(1.,0.,0.));bitangent=cross(n,tangent)*v.tangent.w;
        }else{
            // Invert the UV Jacobian explicitly. Its sign includes UV mirroring
            // and the framebuffer Y direction; dropping it reverses the normal.
            let determinant=uv_dx.x*uv_dy.y-uv_dx.y*uv_dy.x;
            if abs(determinant)>1e-20 {
                tangent=(dx*uv_dy.y-dy*uv_dx.y)/determinant;
                bitangent=(dy*uv_dx.x-dx*uv_dy.x)/determinant;
                tangent=unit(tangent-n*dot(n,tangent),vec3<f32>(1.,0.,0.));
                let handedness=select(-1.,1.,dot(cross(n,tangent),bitangent)>=0.);
                bitangent=cross(n,tangent)*handedness;
            }
        }
        let mapped=vec3<f32>((normal_tex.xy*2.-vec2<f32>(1.))*material.parameters.y,normal_tex.z*2.-1.);
        n=unit(tangent*mapped.x+bitangent*mapped.y+n*mapped.z,n);
    }else if material.flags.z==2u {
        let perp_y=cross(dy,n);let perp_x=cross(n,dx);let determinant=dot(dx,perp_y);
        let gradient=height_dx*perp_y+height_dy*perp_x;
        n=unit(abs(determinant)*n-sign(determinant)*gradient*material.parameters.y*draw.eye_scale.w,n);
    }
    n*=face_sign;
    let view_direction=unit(draw.eye_scale.xyz-v.world,n);
    let base=material.base.rgb*rgb(base_tex,material.maps[0].options.y)*v.color.rgb;
    var alpha=material.base.a*base_tex.a*v.color.a;
    if material.flags.x==1u {alpha*=scalar(opacity_tex,material.maps[5].options.y);}
    if material.flags.y==0u {alpha=1.;}
    else if material.flags.y==1u {if alpha<material.parameters.w {discard;}alpha=1.;}
    let emissive=material.emissive_metallic.rgb*rgb(emissive_tex,material.maps[4].options.y);
    let metallic=material.emissive_metallic.a*detail_tex.b;let roughness=material.parameters.x*detail_tex.g;
    let occlusion=mix(1.,ambient_tex.r,material.parameters.z);
    var radiance=vec3<f32>(0.);
    if material.flags.x==0u {radiance=base*(1.-metallic)*draw.ambient.rgb*occlusion/3.141592653589793;}
    else {
        radiance=material.ambient.rgb*rgb(ambient_tex,material.maps[3].options.y)*draw.ambient.rgb;
        if material.flags.w==0u {return vec4<f32>((base+emissive)*alpha,alpha);}
    }
    for(var i=0u;i<draw.flags.z;i++) {
        let light=draw.lights[i];var direction= -light.position_kind.xyz;var attenuation=1.;
        if light.position_kind.w==1. {
            let delta=light.position_kind.xyz-v.world;let distance=length(delta)/draw.eye_scale.w;direction=unit(delta,n);
            attenuation=1./max(distance*distance,1e-8);
            if light.parameters.x>0. {let relative=distance/light.parameters.x;let cutoff=max(1.-relative*relative*relative*relative,0.);attenuation*=cutoff*cutoff;}
        }
        let incoming=light.color_intensity.rgb*light.color_intensity.a*attenuation;
        if material.flags.x==0u {radiance+=brdf(base,metallic,roughness,n,view_direction,direction)*incoming;}
        else {
            let no_l=max(dot(n,direction),0.);var reflectance=base*no_l;
            if material.flags.w==2u && no_l>0. {
                let exponent=material.specular_exponent.a*scalar(exponent_tex,material.maps[6].options.y);
                let highlight=phong_power(max(dot(reflect(-direction,n),view_direction),0.),exponent);
                reflectance+=material.specular_exponent.rgb*rgb(detail_tex,material.maps[1].options.y)*highlight;
            }
            radiance+=reflectance*incoming;
        }
    }
    return vec4<f32>((radiance+emissive)*alpha,alpha);
}
