// Next-stage FP32 Erf research, not linked, compiled or GPU-qualified yet.
// Mathematical reference only: Abramowitz & Stegun, AMS55 p299 Eq7.1.26,
// https://digital.library.unt.edu/ark:/67531/metadc40302/m1/315/ocr/
// https://dlmf.nist.gov/7.6.E1 (small-argument power series).
// No upstream implementation, image, text passage or table asset is copied.
// The 1.5e-7 approximation bound is for the mathematical rational expression;
// it is NOT a bound already proved for this WGSL/FP32 evaluation.
struct ErfParameters { count_and_dispatch: vec4<u32>, }
@group(0) @binding(0) var<storage, read> erf_input: array<f32>;
@group(0) @binding(3) var<storage, read_write> erf_output: array<f32>;
@group(0) @binding(4) var<uniform> erf_parameters: ErfParameters;

fn erf_candidate(value: f32) -> f32 {
    if (value == 0.0) { return value; }
    let magnitude = abs(value);
    // For real arguments beyond6, the mathematical tail is below FP32
    // resolution. Returning the signed endpoint also avoids squaring huge x.
    if (magnitude >= 6.0) { return sign(value); }
    if (magnitude < 0.5) {
        // Direct series avoids subtracting two nearly equal values near zero.
        var term = value;
        var sum = value;
        for (var n = 1u; n <= 12u; n += 1u) {
            term *= -value * value / f32(n);
            sum += term / f32(2u * n + 1u);
        }
        return 1.1283791670955126 * sum;
    }
    let t = 1.0 / (1.0 + 0.3275911 * magnitude);
    let polynomial = (((((1.061405429 * t - 1.453152027) * t)
        + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t;
    return sign(value) * (1.0 - polynomial * exp(-magnitude * magnitude));
}

@compute @workgroup_size(64, 1, 1)
fn erf_elementwise(@builtin(global_invocation_id) global: vec3<u32>) {
    let index = global.x + global.y * erf_parameters.count_and_dispatch.y;
    if (index < erf_parameters.count_and_dispatch.x) {
        erf_output[index] = erf_candidate(erf_input[index]);
    }
}
