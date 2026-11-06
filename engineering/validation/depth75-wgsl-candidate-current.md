# WGSL depth research: candidate audit and selected operators

Issue #75 remains **incomplete**. This checkpoint inspects the existing,
separately installed Depth Anything V2 Small model and validates four standalone
WGSL operator families against its actual intermediate tensors. It does not
provide composed WGSL model inference, a default product backend, or performance
and photographed/video quality acceptance.

The documentation branch starts at maintained main
`1aa57293a84e70734ff414e9450f1e880c62139d`. Its initial static audit used no GPU
device or learned operator while the separate #88 workspace regression was
active. The later selected-operator execution began after that original full
run and the tracking experiment terminated. No product source, dependency, UI,
model file or default distribution changed.

## Candidate identity and permitted use

The authors, pinned reference revision, external export procedure and licenses
are already recorded under "Optional desktop depth prototype" in
[`ATTRIBUTION.md`](../../ATTRIBUTION.md). The
[pinned official source license](https://github.com/DepthAnything/Depth-Anything-V2/blob/a561b849ebae10a6f5ef49e26c83cbbcd36c71bf/LICENSE)
is Apache-2.0. The
[official model-specific license distinction](https://github.com/DepthAnything/Depth-Anything-V2/blob/a561b849ebae10a6f5ef49e26c83cbbcd36c71bf/README.md#license)
permits Small under Apache-2.0; Base/Large/Giant use CC-BY-NC-4.0 and are not
selected or downloaded. This audit does not authorize packaging their weights.

| Identity | Observed value |
| --- | --- |
| Official reference revision | `a561b849ebae10a6f5ef49e26c83cbbcd36c71bf` |
| Original Small weights SHA-256 | `715fade13be8f229f8a70cc02066f656f2423a59effd0579197bbf57860e1378` |
| Existing exported ONNX SHA-256 | `274368034c0fcef08f9b197d33b606bae9c578acca861ef72feb268143b21434` |
| ONNX file size | 98,941,070 bytes |
| ONNX opset / inspection library | Standard domain opset 17 / separately installed ONNX 1.23.2 |
| Input | Float32 `rgb`, NCHW `[1,3,518,518]` |
| Output | Float32 `relative_inverse_depth`, `[1,518,518]` |
| Initializers | 234 Float32 tensors, 24,710,849 stored elements, 98,843,396 bytes |

The [official model card](https://huggingface.co/depth-anything/Depth-Anything-V2-Small)
describes training with synthetic labeled and real unlabeled images. That makes
it a broader-domain baseline than a driving-only candidate; it does not prove
suitability for this editor's footage. The output is relative inverse depth,
not metres, confidence, a temporally stable sequence or physical ground truth.
Its "Small" name is not a measured lightweight/interactive qualification.

## Actual operator and shape inventory

All 598 nodes belong to the standard ONNX domain; there are 23 operation kinds.
The weights, input and learned data path remain Float32. Integer/boolean
shape-control tensors are retained. No quantization operators were found; this
checkpoint does not provide an int8 or Float16 model.

| Operation | Nodes | WGSL implementation boundary still requiring proof |
| --- | ---: | --- |
| MatMul | 72 | Batched attention/projection layouts, transpose/broadcast semantics and accumulation error |
| Conv / ConvTranspose | 31 / 2 | Bias, layout, padding and stride; transpose kernels use 2x2/stride 2 and 4x4/stride 4 |
| LayerNormalization | 28 | Axis -1, epsilon approximately 1e-6, reduction and variance precision |
| Softmax | 12 | Axis -1; numerically stable reductions, not independent elementwise exponentials |
| Erf | 12 | GELU's error-function operation needs a bounded numerical implementation |
| Resize | 5 | Linear interpolation with **align_corners**; substituting half-pixel sampling changes results |
| Add / Mul / Div / Relu | 95 / 61 / 12 / 17 | Exact broadcasting, layout and numerical comparison |
| Transpose / Reshape / Gather / Slice / Concat / Squeeze | 41 / 29 / 36 / 9 / 6 / 1 | View-versus-materialization and index handling, not six unconditional copy kernels |
| Constant / ConstantOfShape / Shape / Equal / Where / Expand | 120 / 1 / 5 / 1 / 1 / 1 | Fixed-shape planning may eliminate work, but that elimination is not implemented here |

The first shape-inference pass left 242 tensor sizes unresolved. ONNX's static
shape-data propagation reduced that to two; an input-free seven-node integer
shape program resolved the remaining Expand/Concat dimensions to `[1,1,384]`
and `[1,1370,384]`. Its three-element result was cross-checked against the
independently inferred downstream Add shape. No image or learned weights were
evaluated by that shape program. The final inventory has zero unknown sizes.
All three stages are retained so the initial incomplete estimate cannot be
mistaken for the final one.

## Static memory and device limits

A naive serial materialization schedule allocates each node's outputs before
releasing its last-use inputs. Its peak activation storage is **100,612,800
bytes**, at node 406, a Softmax. Independent inclusive producer/last-use
interval enumeration reproduces the same peak and node. Persistent weights
and Constant outputs add **98,844,892 bytes**; their combined static amount is
**199,457,692 bytes**, approximately 190.2 MiB.

This schedule has no aliasing, fusion, tiling, arena alignment or allocator
reuse. It also excludes upload/readback staging, host/model-loader copies,
pipelines, bindings and the editor's other GPU resources. The number is neither
a measured process/device peak nor a lower bound for every optimized execution.

The largest nonconstant tensor is attention storage `[1,6,1370,1370]`,
**45,045,600 bytes**. For comparison, the
[WebGPU core limits](https://www.w3.org/TR/2026/CRD-webgpu-20260915/#limits)
include a 128 MiB storage-buffer binding, 256 MiB buffer, 16 KiB workgroup
storage and 256 invocations per workgroup. These are comparison baselines,
not queried capabilities of the current adapter or a promise for compatibility
adapters. Every actual device must be checked before allocation/dispatch.
Being smaller than one binding limit does not establish available memory,
valid workgroup dimensions, execution time or thermal stability.

## Retained evidence and remaining requirements

The [archive](data/depth75-static-model-audit-v1.zip) and
[manifest](data/depth75-static-model-audit-v1.json) contain the independently
authored controllers, all three static inventories, the second lifetime check
and exact operator-attribute variants. All 12 members were read back and
verified: 40,185 bytes, SHA-256
`668341c2ba2c18778d32b31109928976d946656117cd7a75c481503dbbda2d5b`.
No model, weights, upstream source, photographs or dataset payload is included.
The controllers are frozen local observers with explicit local input/output
roots; reproducing elsewhere requires recording that routing change and using
the same separately exported model hash and optional environment.

| Original #75 requirement | Current disposition |
| --- | --- |
| I1: licensed candidate, domain, operators/shapes/dtypes, weights/activation/device limits | Pinned candidate and complete static inventory recorded; deployment suitability and lightweight performance remain unqualified |
| I2: same-weight per-operator and end-to-end WGSL/ONNX comparison | Four WGSL families passed 13 actual-model representatives and 21 edge controls; remaining families and composed inference pending |
| I3: shared DepthResource/job contract; quality independent of backend | Existing ONNX route is separate evidence; adding a second execution backend remains pending |
| I4: measured quality/backend resolution and explicit capability/model/operator fallback | Pending; no automatic backend selection is introduced |
| I5: fast/high transitions, versioned alignment, hysteresis/generation and frozen export policy | Pending dual-backend integration and independent video tests |
| I6: versioned compute-plan/weight resources and explicit execution capability negotiation | Pending; do not silently accept an unsupported plan or bypass the current msfx compute restriction |
| A1: license qualification before default distribution | Candidate source/model distinction checked; default distribution is not enabled |
| A2: same-model error, whole-pipeline latency, weights/activations and thermal record | Static sizes and selected-operator numerical evidence recorded; composed-model error, latency, actual memory and thermal measurements pending |
| A3: explicit failure/fallback and no stale background result overwrite | Requires the real second backend, failure injection and generation tests |
| A4: no additional scale drift or depth jumps when switching video quality | Pending; single-frame inventory cannot prove temporal behavior |

Android is retired and excluded by the user's maintenance scope. That exclusion
does not waive desktop numerical, scheduling, memory, temporal or output tests.
Issue #75 and the complete 3D route remain open.

## Actual selected-operator GPU qualification

An independently authored standalone Rust/WGSL probe executed tiled batched
MatMul, stable final-axis Softmax, final-axis LayerNormalization and NCHW linear
Resize with the model's exact `align_corners` rule. Host admission checks model
identity, tensor SHA-256/length/finiteness, shape, broadcast/layout semantics,
dispatch/device limits and a bounded logical buffer budget before submission.
It requests an explicit Vulkan hardware adapter with Float32 arithmetic and no
backend or precision fallback.

The existing Small ONNX export ran once with CPUExecutionProvider, one thread
and graph optimization disabled to retain actual named intermediate inputs and
weights. One representative of each of the 13 distinct audited layouts covers
the four families' 117 static nodes: six MatMul layouts, one normalization,
one Softmax and five Resize layouts. This samples 13 actual nodes; it does not
prove every weight/activation or compose those operations into inference.
Another 21 independently authored Float64-reference-to-Float32 controls cover
partial tiles, shared batches, reduction boundaries, constant rows, extreme
logits and singleton/up/downsample axes. Model and tensor payloads stay external.

| Executed gate | Result |
| --- | --- |
| Host admission tests | All 15 passed; ten first-family and five convolution-layout controls |
| Explicit hardware | NVIDIA GeForce RTX 4070 Laptop GPU, Vulkan, driver 591.86 |
| Actual same-model representatives | All 13 passed |
| Authored numerical edge controls | All 21 passed |
| Predeclared normalized error bounds | Maximum 0.0002; mean 0.00002; unchanged after execution |
| Largest measured normalized errors across cases | Maximum 0.000005754542; largest case mean 0.000000043629 |
| Actual negative requests | Wrong model, unsupported axis, exhausted budget, tensor hash mismatch and nonfinite input all refused with original causes |
| Default logging and protocol channels | Info starts visible; all five Error causes in file and stderr; stdout empty |

The independent observer checks identities, complete output sizes and finite
values. It divides error only by `max(max(abs(reference)), 1e-6)`, with no fitted
output scale or affine alignment. Its own corrupted-value, NaN and wrong-length
controls are rejected. The actual GPU stage took 12.891 seconds including cold
per-operator allocations/submits/readback across all cases. This is not a
whole-model latency benchmark or thermal/memory qualification.

Two preceding attempts remain retained. The first failed compilation on three
probe API errors. The second passed all 13 model cases but failed the constant
width-seven normalization edge case: mean normalized error 0.000027934745 was
above the original 0.00002 bound. The third centers each row relative to its
first element before mean/variance reduction; constant rows then remain exactly
zero. Reference bytes, tests, tolerances and budgets were unchanged. No failed
attempt was overwritten or counted as passing acceptance.

The [source and execution archive](data/depth75-selected-operators-v3.zip) and
[manifest](data/depth75-selected-operators-v3.json) preserve Rust/WGSL sources,
Cargo.lock, host tests, fixture/observer controllers, default-level logs,
all three attempts and separate static/scalar diagnostics. All members were
read back: 203,580 bytes, SHA-256
`05154001c63784902689bc8a5034c68dcbe7bbd9895ce1c81f82e50c2ad34ff7`.
Weights, RGB, learned activations, raw output/reference tensors, executables and
optional runtimes are excluded. The included convolution and Erf shaders are
unlinked candidates: host-layout/scalar checks do not qualify their GPU output.
Published mathematical references for Erf are recorded in `ATTRIBUTION.md` and
`PROVENANCE.md`; no reference source code, scans or text are copied.

This checkpoint supplies selected numerical primitives, not a selectable
product backend, complete model, video quality switch, resource planner or
desktop cross-platform acceptance. Those issue requirements remain open.
