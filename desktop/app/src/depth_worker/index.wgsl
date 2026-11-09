// Independent rank-five materialization research. Device execution is pending.
struct IndexParameters { words: array<vec4<u32>,16>, }
@group(0) @binding(0) var<storage,read> a:array<f32>;
@group(0) @binding(1) var<storage,read> b:array<f32>;
@group(0) @binding(2) var<storage,read> c:array<f32>;
@group(0) @binding(3) var<storage,read_write> output:array<f32>;
@group(0) @binding(4) var<uniform> parameters:IndexParameters;
fn word(index:u32)->u32 {return parameters.words[index/4u][index%4u];}
fn coordinates(index:u32)->array<u32,5> {
    var result:array<u32,5>;var remaining=index;
    for(var reversed=0u;reversed<5u;reversed+=1u) {
        let axis=4u-reversed;let extent=word(4u+axis);
        result[axis]=remaining%extent;remaining/=extent;
    }
    return result;
}
fn address(input:u32,position:array<u32,5>)->u32 {
    let slot=12u+input*8u;var result=bitcast<i32>(word(48u+input));
    for(var axis=0u;axis<5u;axis+=1u) {
        result+=i32(position[axis])*bitcast<i32>(word(slot+axis));
    }
    return u32(result);
}
@compute @workgroup_size(64,1,1)
fn index_copy(@builtin(global_invocation_id) global:vec3<u32>) {
    let i=global.x+global.y*word(1u);if(i>=word(0u)){return;}
    output[i]=a[address(0u,coordinates(i))];
}
@compute @workgroup_size(64,1,1)
fn binary_add(@builtin(global_invocation_id) global:vec3<u32>) {
    let i=global.x+global.y*word(1u);if(i>=word(0u)){return;}
    let p=coordinates(i);output[i]=a[address(0u,p)]+b[address(1u,p)];
}
@compute @workgroup_size(64,1,1)
fn binary_mul(@builtin(global_invocation_id) global:vec3<u32>) {
    let i=global.x+global.y*word(1u);if(i>=word(0u)){return;}
    let p=coordinates(i);output[i]=a[address(0u,p)]*b[address(1u,p)];
}
@compute @workgroup_size(64,1,1)
fn binary_div(@builtin(global_invocation_id) global:vec3<u32>) {
    let i=global.x+global.y*word(1u);if(i>=word(0u)){return;}
    let p=coordinates(i);output[i]=a[address(0u,p)]/b[address(1u,p)];
}
@compute @workgroup_size(64,1,1)
fn relu(@builtin(global_invocation_id) global:vec3<u32>) {
    let i=global.x+global.y*word(1u);if(i>=word(0u)){return;}
    output[i]=max(0.0,a[address(0u,coordinates(i))]);
}
@compute @workgroup_size(64,1,1)
fn concat(@builtin(global_invocation_id) global:vec3<u32>) {
    let i=global.x+global.y*word(1u);if(i>=word(0u)){return;}
    var p=coordinates(i);let axis=word(42u);let first=word(43u);let second=word(44u);
    if(p[axis]<first) {output[i]=a[address(0u,p)];}
    else if(p[axis]<first+second) {p[axis]-=first;output[i]=b[address(1u,p)];}
    else {p[axis]-=first+second;output[i]=c[address(2u,p)];}
}
