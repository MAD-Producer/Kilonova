# Independent pointwise fusion obligations

Issue #78 inherits the opt-in Invert/Tint prototype from #73. Each obligation
below is separate. An implementation fingerprint, a planning test, a passing
pixel fixture, or a successful build does not discharge the other obligations.
Android is outside this delivery. Final-source receipts and maintained desktop
CI are recorded in `typed-render-dag-current.md`; this document describes the
argument and its executable controls, not an assertion that pending gates passed.

## 1. Shader operations and implementation identity

The admitted scalar operations are the pinned built-in Invert and Tint bodies.
Invert changes the selected RGB component(s), or alpha, and mixes with its
original using the declared blend parameter. Tint computes the documented
weighted RGB dot product, mixes the two declared colors by that value, then
mixes with the original RGB by amount while preserving alpha. Generated
functions preserve the same expressions and parameter order; they neither
clamp HDR values nor replace floating arithmetic with a different color model.

`shader_operation_equivalence_uses_pinned_original_bodies_and_generated_gpu_functions`
extracts the actual original operation bodies and the actual generated scalar
functions, supplies the same GPU input/uniforms, and requires bitwise equality.
The independently authored finite grid covers negative values, signed zero,
binary16/subnormal scales, ordinary colors and HDR. All supported Invert
channels and multiple blend/amount values are exercised. Mutating compiled
code while preserving a recognized name/hash must lose eligibility.

Production admission separately checks the frozen source digest, complete
definition, parameter layout, both compiled backend representations and actual
selected native program. Exact pure-operation evaluation does not prove the
SDK's conversions, stores, alpha lifting, sampling, or consumers.

## 2. Quantization and ordered storage boundaries

For every admitted stage, let C be the canonical pixel, K the conversion to
working color/alpha, Qw a working store, F the pinned scalar function, t the
stage opacity, and Qc the canonical store. The unfused recurrence is

`C_next = Qc(K_inverse(Qw(mix(Qw(K(C)), F(Qw(K(C))), t))))`.

Generated code preserves this ordered recurrence. It emulates every removed
working and intermediate canonical store; the final output attachment performs
the terminal canonical store. Storage precision and color/alpha modes belong
to the group identity. Omitting the stores or merely clamping the final output
is not equivalent to this recurrence.

`quantization_half_gpu_function_checks_every_finite_cell_and_ties_against_independent_centers`
constructs binary16 centers independently using binary exponents and wide CPU
arithmetic. It checks every signed finite half cell, including centers, quarter,
halfway and three-quarter positions, signed zero, subnormals and overflow to
infinity. The actual generated GPU function must match nearest-even output
bits. A GPU control that omits quantization must differ. This establishes the
software binary16 quantizer, not native attachment rounding on every adapter.

Native floating-point format conversion does not guarantee nearest-even:
[Vulkan floating-point format conversions](https://github.khronos.org/Vulkan-Site/spec/latest/chapters/fundamentals.html#fundamentals-fp-conversion).
The native Float16 effect-output policy must therefore explicitly select the
same representable values as removed stores. Its specialization is selected by
the admitted output attachment format and participates in the native pipeline
format key. The full shared helper is part of compiled source/cost accounting.
Hardware blend stages remain outside this pure effect-store argument.

UNORM8 includes its sRGB canonical attachment conversion; UNORM16 has a linear
canonical attachment; Float32 has no removed reduced-precision store. Native
whole-chain comparisons retain their declared numeric bounds. Those bounded
finite probes do not establish bit identity for all representable inputs.
The independently retained UNORM16 Tint-before-Invert counterexample must keep
its actual stores; reverse order supplies a positive admission control.

## 3. Alpha and the unproved alpha-changing boundary

RGB-only Invert and Tint preserve the stage's alpha. For an admitted group,
the native positive probe requires alpha equality independently of RGB error,
including zero alpha, values below the SDK straight-alpha threshold, ordinary
alpha and HDR RGB. The source is procedural, not restricted to the PNG grid.
This verifies the alpha channel separately from the scalar RGB proof.

The SDK straight conversion discards hidden RGB below its explicit alpha
threshold. Changing alpha after those stores can reveal or amplify information
loss. Invert's alpha channel therefore retains every original pass in the draw,
including earlier otherwise eligible stages. The existing alpha-lifting probe
requires exact original pixels, zero grouped shaders and an AlphaBoundary
reason across four precisions and eight Tint amounts. This is a proved
conservative boundary, not a claim that alpha-changing fusion is supported.

## 4. External image and matte readers

An effected-layer resource or matte consumer observes the producer's stored
image, not just its final on-screen contribution. A local final-output bound
cannot be reused as a proof for that additional consumer. The owned semantic
graph supplies these producer/consumer edges before native admission. Actual
effected-image and matte readers retain all producer stores; resource-stage
identity distinguishes source reads from effected reads.

`effected_resource_and_matte_consumers_keep_the_producers_real_stores` exercises
both native consumer families in all four precisions and requires exact
whole-image output and the specific retention reason. Independent semantic
graph tests verify the source/effected edge distinction. This argument is
identity by unchanged pass sequence, not a shader-only proof.

## 5. Compositing and backdrop consumers

Non-normal transfer blends, non-linear compositing spaces and adjustment-layer
backdrops can amplify even a bounded local source difference. Adjustment
backdrops also have implicit accumulated-scene dependencies. The prototype
therefore retains the original pass sequence when these visible consumers are
present. This is conservative over the frame, including unrelated candidates;
narrower admission needs another proof.

`transfer_blends_and_adjustment_backdrops_retain_unproved_compositing_boundaries`
requires exact native output in four precisions for all three consumer families,
zero affected grouping and a CompositingBoundary reason. These controls establish
preservation through retention. They do not authorize fusion into non-normal
blending, adjustment layers, blur/warp, AI, 3D, format/ROI changes or stateful
operations. The ordinary whole-chain probe supplies a separate positive case.

## Acceptance boundary

Five independently named gates produce separate raw logs and artifacts from
the same frozen source. Failures, including the new HDR Float16 counterexample,
remain evidence. Existing tolerances and assertions must not be loosened to
declare completion. Final acceptance also needs the current main rebase,
whole-workspace regression and maintained desktop CI on the resulting source.
