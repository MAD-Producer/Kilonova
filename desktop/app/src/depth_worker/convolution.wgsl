// Independently authored next-stage convolution kernels. Not linked into the
// first four-operator probe and not compiled or qualified yet.
// The host must admit NCHW, group1, dilation1, OIHW/IOHW weight layouts,
// exact output dimensions, all buffer lengths, optional bias and dispatch.
struct ConvParameters {
    input_size: vec4<u32>, // N,C,H,W
    output_size: vec4<u32>, // output channels,height,width,kernel height
    kernel: vec4<u32>, // kernel width,stride height,stride width,pad top
    dispatch: vec4<u32>, // pad left,flat X stride,bias present,0
}
@group(0) @binding(0) var<storage, read> image: array<f32>;
@group(0) @binding(1) var<storage, read> weights: array<f32>;
@group(0) @binding(2) var<storage, read> bias: array<f32>;
@group(0) @binding(3) var<storage, read_write> convolution_output: array<f32>;
@group(0) @binding(4) var<uniform> convolution: ConvParameters;

// ONNX Conv is cross-correlation, without spatially reversing the OIHW weights.
// Boundary padding contributes zero; it never clamps into an edge image pixel.
@compute @workgroup_size(64, 1, 1)
fn conv_nchw(@builtin(global_invocation_id) global: vec3<u32>) {
    let ci = convolution.input_size.y;
    let ih = convolution.input_size.z;
    let iw = convolution.input_size.w;
    let co = convolution.output_size.x;
    let oh = convolution.output_size.y;
    let ow = convolution.output_size.z;
    let kh = convolution.output_size.w;
    let kw = convolution.kernel.x;
    let index = global.x + global.y * convolution.dispatch.y;
    if (index >= convolution.input_size.x * co * oh * ow) { return; }
    let ox = index % ow;
    let oy = (index / ow) % oh;
    let oc = (index / (ow * oh)) % co;
    let batch = index / (ow * oh * co);
    var value = 0.0;
    if (convolution.dispatch.z != 0u) { value = bias[oc]; }
    for (var ic = 0u; ic < ci; ic += 1u) {
        for (var ky = 0u; ky < kh; ky += 1u) {
            let iy = i32(oy * convolution.kernel.y + ky) - i32(convolution.kernel.w);
            if (iy < 0 || iy >= i32(ih)) { continue; }
            for (var kx = 0u; kx < kw; kx += 1u) {
                let ix = i32(ox * convolution.kernel.z + kx) - i32(convolution.dispatch.x);
                if (ix < 0 || ix >= i32(iw)) { continue; }
                let source = ((batch * ci + ic) * ih + u32(iy)) * iw + u32(ix);
                let weight = ((oc * ci + ic) * kh + ky) * kw + kx;
                value += image[source] * weights[weight];
            }
        }
    }
    convolution_output[index] = value;
}

// The audited Small model has exactly two nonoverlapping transpose convolutions:
// kernel2/stride2 and kernel4/stride4, group1, dilation1, padding/output_padding0.
// IOHW weights are deliberately indexed differently from Conv's OIHW layout.
// This entry point MUST reject any other host request; it is not a generic
// overlapping transposed convolution, and uses no scatter/float atomics.
@compute @workgroup_size(64, 1, 1)
fn conv_transpose_no_overlap_nchw(@builtin(global_invocation_id) global: vec3<u32>) {
    let ci = convolution.input_size.y;
    let ih = convolution.input_size.z;
    let iw = convolution.input_size.w;
    let co = convolution.output_size.x;
    let oh = convolution.output_size.y;
    let ow = convolution.output_size.z;
    let kh = convolution.output_size.w;
    let kw = convolution.kernel.x;
    let index = global.x + global.y * convolution.dispatch.y;
    if (index >= convolution.input_size.x * co * oh * ow) { return; }
    let ox = index % ow;
    let oy = (index / ow) % oh;
    let oc = (index / (ow * oh)) % co;
    let batch = index / (ow * oh * co);
    let ky = oy % kh;
    let kx = ox % kw;
    let iy = oy / kh;
    let ix = ox / kw;
    var value = 0.0;
    if (convolution.dispatch.z != 0u) { value = bias[oc]; }
    for (var ic = 0u; ic < ci; ic += 1u) {
        let source = ((batch * ci + ic) * ih + iy) * iw + ix;
        let weight = ((ic * co + oc) * kh + ky) * kw + kx;
        value += image[source] * weights[weight];
    }
    convolution_output[index] = value;
}
