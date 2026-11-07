// Independently authored FP32 operator research, not an installed backend.
// Logical tensors are contiguous. Host admission validates all dimensions,
// byte sizes, strides, dispatch bounds and finite inputs before submission.
struct Parameters {
    dimensions: vec4<u32>,
    strides: vec4<u32>,
    options: vec4<u32>,
    reserved: vec4<u32>,
}
@group(0) @binding(0) var<storage, read> a: array<f32>;
@group(0) @binding(1) var<storage, read> b: array<f32>;
@group(0) @binding(2) var<storage, read> c: array<f32>;
@group(0) @binding(3) var<storage, read_write> output: array<f32>;
@group(0) @binding(4) var<uniform> parameters: Parameters;

var<workgroup> tile_a: array<f32, 64>;
var<workgroup> tile_b: array<f32, 64>;
var<workgroup> reduction: array<f32, 64>;

// dimensions=(M,N,K,batches), strides=(A batch stride,B batch stride,0,0).
// Zero batch stride explicitly broadcasts a shared matrix; no hidden reshape.
@compute @workgroup_size(8, 8, 1)
fn matmul(@builtin(global_invocation_id) global: vec3<u32>,
          @builtin(local_invocation_id) local: vec3<u32>) {
    let row = global.y;
    let column = global.x;
    let m = parameters.dimensions.x;
    let n = parameters.dimensions.y;
    let k = parameters.dimensions.z;
    let a_start = global.z * parameters.strides.x;
    let b_start = global.z * parameters.strides.y;
    let lane = local.y * 8u + local.x;
    var value = 0.0;
    for (var start = 0u; start < k; start += 8u) {
        let a_k = start + local.x;
        let b_k = start + local.y;
        tile_a[lane] = 0.0;
        tile_b[lane] = 0.0;
        if (row < m && a_k < k) {
            tile_a[lane] = a[a_start + row * k + a_k];
        }
        if (b_k < k && column < n) {
            tile_b[lane] = b[b_start + b_k * n + column];
        }
        // Boundary lanes also participate in both barriers.
        workgroupBarrier();
        for (var inner = 0u; inner < 8u; inner += 1u) {
            value += tile_a[local.y * 8u + inner] * tile_b[inner * 8u + local.x];
        }
        workgroupBarrier();
    }
    if (row < m && column < n) {
        output[(global.z * m + row) * n + column] = value;
    }
}

// dimensions=(rows,width,0,0), reserved.y=workgroups in dispatch X.
// The final axis is contiguous and normalized; row guards are workgroup-uniform.
@compute @workgroup_size(64, 1, 1)
fn softmax(@builtin(workgroup_id) group: vec3<u32>,
           @builtin(local_invocation_id) local: vec3<u32>) {
    let row = group.x + group.y * parameters.reserved.y;
    if (row >= parameters.dimensions.x) { return; }
    let width = parameters.dimensions.y;
    let lane = local.x;
    var maximum = -3.402823466e+38;
    for (var column = lane; column < width; column += 64u) {
        maximum = max(maximum, a[row * width + column]);
    }
    reduction[lane] = maximum;
    workgroupBarrier();
    for (var stride = 32u; stride > 0u; stride /= 2u) {
        if (lane < stride) {
            reduction[lane] = max(reduction[lane], reduction[lane + stride]);
        }
        workgroupBarrier();
    }
    maximum = reduction[0];
    workgroupBarrier();
    var total = 0.0;
    for (var column = lane; column < width; column += 64u) {
        total += exp(a[row * width + column] - maximum);
    }
    reduction[lane] = total;
    workgroupBarrier();
    for (var stride = 32u; stride > 0u; stride /= 2u) {
        if (lane < stride) {
            reduction[lane] += reduction[lane + stride];
        }
        workgroupBarrier();
    }
    total = reduction[0];
    for (var column = lane; column < width; column += 64u) {
        output[row * width + column] = exp(a[row * width + column] - maximum) / total;
    }
}

// dimensions=(rows,width,0,0), options.x=the exact positive FP32 epsilon bits.
// B/C contain the width-element scale and bias from the same ONNX model.
@compute @workgroup_size(64, 1, 1)
fn layer_norm(@builtin(workgroup_id) group: vec3<u32>,
              @builtin(local_invocation_id) local: vec3<u32>) {
    let row = group.x + group.y * parameters.reserved.y;
    if (row >= parameters.dimensions.x) { return; }
    let width = parameters.dimensions.y;
    let lane = local.x;
    // Translation-invariant centering keeps a constant row exactly zero,
    // including widths whose reciprocal is not exactly representable in FP32.
    // It also avoids accumulating a large common offset before subtraction.
    let origin = a[row * width];
    var total = 0.0;
    for (var column = lane; column < width; column += 64u) {
        total += a[row * width + column] - origin;
    }
    reduction[lane] = total;
    workgroupBarrier();
    for (var stride = 32u; stride > 0u; stride /= 2u) {
        if (lane < stride) {
            reduction[lane] += reduction[lane + stride];
        }
        workgroupBarrier();
    }
    let mean = reduction[0] / f32(width);
    workgroupBarrier();
    var variance = 0.0;
    for (var column = lane; column < width; column += 64u) {
        let centered = (a[row * width + column] - origin) - mean;
        variance += centered * centered;
    }
    reduction[lane] = variance;
    workgroupBarrier();
    for (var stride = 32u; stride > 0u; stride /= 2u) {
        if (lane < stride) {
            reduction[lane] += reduction[lane + stride];
        }
        workgroupBarrier();
    }
    let inverse_scale = inverseSqrt(reduction[0] / f32(width) + bitcast<f32>(parameters.options.x));
    for (var column = lane; column < width; column += 64u) {
        output[row * width + column] = ((a[row * width + column] - origin) - mean) * inverse_scale * b[column] + c[column];
    }
}

// dimensions=(N,C,input height,input width), strides=(output height,output width,0,0),
// reserved.x=dispatch X * 64. Large feature maps use a bounded 2D dispatch.
// ONNX Resize linear/align_corners, including the single-pixel output case.
@compute @workgroup_size(64, 1, 1)
fn resize_align_corners(@builtin(global_invocation_id) global: vec3<u32>) {
    let ih = parameters.dimensions.z;
    let iw = parameters.dimensions.w;
    let oh = parameters.strides.x;
    let ow = parameters.strides.y;
    let size = parameters.dimensions.x * parameters.dimensions.y * oh * ow;
    let index = global.x + global.y * parameters.reserved.x;
    if (index >= size) {
        return;
    }
    let x = index % ow;
    let y = (index / ow) % oh;
    let plane = index / (ow * oh);
    var sx = 0.0;
    var sy = 0.0;
    if (ow > 1u) { sx = f32(x) * f32(iw - 1u) / f32(ow - 1u); }
    if (oh > 1u) { sy = f32(y) * f32(ih - 1u) / f32(oh - 1u); }
    let x0 = min(u32(floor(sx)), iw - 1u);
    let y0 = min(u32(floor(sy)), ih - 1u);
    let x1 = min(x0 + 1u, iw - 1u);
    let y1 = min(y0 + 1u, ih - 1u);
    let wx = sx - f32(x0);
    let wy = sy - f32(y0);
    let start = plane * ih * iw;
    let upper = a[start + y0 * iw + x0] * (1.0 - wx) + a[start + y0 * iw + x1] * wx;
    let lower = a[start + y1 * iw + x0] * (1.0 - wx) + a[start + y1 * iw + x1] * wx;
    output[index] = upper * (1.0 - wy) + lower * wy;
}
