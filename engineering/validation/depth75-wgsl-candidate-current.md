# WGSL depth research: candidate, operators and composed inference

Issue #75 remains **incomplete**. The latest standalone research execution
composes the pinned Depth Anything V2 Small model on a hardware GPU and compares
its final depth against the same-weight CPU reference. It passes the original
numerical bounds on one authored input. Shared product-backend integration,
video quality transitions, complete pipeline latency, thermal and other desktop
GPU qualification remain pending. Earlier sections retain the static and
selected-operator checkpoints with their original, narrower evidence scopes.

The original documentation branch starts at maintained main
`1aa57293a84e70734ff414e9450f1e880c62139d`. Its initial static audit used no GPU
device or learned operator while the separate #88 workspace regression was
active. The later selected-operator execution began after that original full
run and the tracking experiment terminated. No product source, dependency, UI,
model file or default distribution changed. The composed follow-up starts at
maintained main `daf806b5f29c2ec0a70e41068445230fb7b13585`, after PR #623 passed
all eight applicable CI checks and merged. Its numerical source is frozen in
the archive below; green repository CI does not execute that external model.

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

| Operation | Nodes | Audited implementation boundary |
| --- | ---: | --- |
| MatMul | 72 | Batched attention/projection layouts, transpose/broadcast semantics and accumulation error |
| Conv / ConvTranspose | 31 / 2 | Bias, layout, padding and stride; transpose kernels use 2x2/stride 2 and 4x4/stride 4 |
| LayerNormalization | 28 | Axis -1, epsilon approximately 1e-6, reduction and variance precision |
| Softmax | 12 | Axis -1; numerically stable reductions, not independent elementwise exponentials |
| Erf | 12 | GELU's error-function operation needs a bounded numerical implementation |
| Resize | 5 | Linear interpolation with **align_corners**; substituting half-pixel sampling changes results |
| Add / Mul / Div / Relu | 95 / 61 / 12 / 17 | Exact broadcasting, layout and numerical comparison |
| Transpose / Reshape / Gather / Slice / Concat / Squeeze | 41 / 29 / 36 / 9 / 6 / 1 | View-versus-materialization and index handling, not six unconditional copy kernels |
| Constant / ConstantOfShape / Shape / Equal / Where / Expand | 120 / 1 / 5 / 1 / 1 / 1 | Initial inventory only; the later fixed-shape research planner resolves controls and materializes the required Float32 Expand |

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
| I2: same-weight per-operator and end-to-end WGSL/ONNX comparison | Earlier 85 comparisons retained; 55 indexing/broadcast controls and one composed 429-pass model execution now pass; only one authored whole-model input and one hardware adapter are qualified |
| I3: shared DepthResource/job contract; quality independent of backend | Existing ONNX route is separate evidence; adding a second execution backend remains pending |
| I4: measured quality/backend resolution and explicit capability/model/operator fallback | Pending; no automatic backend selection is introduced |
| I5: fast/high transitions, versioned alignment, hysteresis/generation and frozen export policy | Pending dual-backend integration and independent video tests |
| I6: versioned compute-plan/weight resources and explicit execution capability negotiation | Strict version-1 research blueprint and typed admission now tested; product package/capability negotiation remains pending; msfx compute restrictions unchanged |
| A1: license qualification before default distribution | Candidate source/model distinction checked; default distribution is not enabled |
| A2: same-model error, whole-pipeline latency, weights/activations and thermal record | Same-model composed error and 227,968,156 retained buffer-capacity bytes measured; one fenced submission/readback wall observation recorded; preprocessing/remap/transport, physical VRAM, sustained latency and thermal qualification pending |
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

## Expanded Conv, ConvTranspose and Erf qualification

The next frozen attempt links three more independently authored shader entries:
NCHW/OIHW Conv cross-correlation with explicit padding/stride and optional bias,
NCHW/IOHW nonoverlapping ConvTranspose, and Float32 Erf. The transpose contract
admits only the audited unpadded 2/stride-2 and 4/stride-4 forms with group one,
dilation one and no output padding; it refuses overlapping requests rather than
silently approximating them.

The same pinned CPU model ran once more with identical authored RGB bytes,
thread/provider/optimization options and final depth hash. All 31 Conv, two
ConvTranspose and 12 Erf nodes supplied their actual inputs, weights and
reference outputs. These 45 cases join every prior case. Five independently
authored Float64 convolution references additionally exercise rectangular
asymmetric padding, stride, batch/channel indexing, absent bias, patch-14
embedding and both transpose kernels. Another Erf reference evaluates 20,025
arguments, including signed zero, subnormal/small inputs, both sides of the
series boundary, saturation and the largest finite Float32 arguments.

| Expanded executed gate | Result |
| --- | --- |
| Complete host admission suite | 17 passed, none ignored or filtered |
| Actual same-model node comparisons | All 58 passed; the earlier 13 plus all 45 newly selected nodes |
| Authored numerical controls | All 27 passed; the earlier 21 plus six new controls |
| Original normalized bounds | Maximum 0.0002 and mean 0.00002 retained for every case |
| Largest measured normalized errors | Maximum 0.000005754542; largest case mean 0.000000166635 |
| Additional Erf gate, fixed before execution | Maximum absolute 0.000001; measured 0.000000357628 across actual-node and authored inputs |
| Real failure requests | All seven refused: original five plus unsupported transpose overlap and Erf shape mismatch |
| Default channels | Info starts visible; seven original Error causes in file and stderr; stdout empty |

The shader's Erf approximation combines an independently implemented
small-argument power series and the published mathematical expression credited
in the provenance ledgers. The separate scalar check did not substitute for
this actual GPU comparison. The mathematical expression's ideal error bound
alone is not a Float32 hardware guarantee.

The [expanded source and execution archive](data/depth75-selected-operators-v4.zip)
and [manifest](data/depth75-selected-operators-v4.json) retain the exact sources,
Cargo.lock, tests, independent reference/observer controllers, hashes, actual
logs and preceding qualification identity. All members were read back:
142,002 bytes, SHA-256
`40f32344bd0d857ac02097b042dfe0d4a7d9d6900ac7a771ea3dc092caff8727`.
The same model/weight/tensor/binary exclusions apply. Preparatory source comments
about unlinked kernels refer to the earlier snapshot; this later attempt
actually executed all seven entries. That independent-operator execution does not compose
the 598-node graph, qualify a resource arena, or establish product/backend
selection, temporal consistency, full inference performance or other desktops.

## Composed whole-model GPU qualification

The new independently authored planner partitions all 598 original nodes into
90 fixed integer controls, 30 contiguous aliases, 49 persistent Float32
constants and 429 materialized Float32 operations. The latter comprise 162
instances of the seven previously qualified numerical families and 267
index/broadcast operations. Host lowering resolves controls without evaluating
learned floating-point operators. It independently validates the exact model,
resources, dimensions, operator contracts, data dependencies, aliases and
inclusive producer/last-read lifetimes before device allocation or submission.

The first two allocator attempts refused the unchanged 256 MiB capacity bound;
both failures remain in the archive. The third uses offline descending-capacity
interval coloring. Seven planner controls and an independent audit of 817
reads, 430 owners and 860 lifetime events pass. The Rust admission layer
recomputes graph ownership independently instead of trusting Python's intervals.
It also refuses a slot that overlaps the current input or a final output that
would be overwritten before readback.

The missing index entries implement bounded rank-five broadcasting, Relu,
Expand, Transpose, scalar Gather, nonempty signed Slice and two/three-input
Concat. All 31 distinct layouts used by the 267 actual-model nodes and 24
independently authored controls pass. Nonarithmetic copy/index/Relu results
also require exact numerical value equality; signed zeros compare equally.
All 31 host controls pass without ignored or filtered tests: the original 17
plus seven indexing and seven graph-admission controls. Prior numerical
sources, tests, logger and Cargo.lock remain byte-identical; Cargo.toml adds
only the research binaries and test targets.

The actual runtime uploads each externally retained, hash-checked finite
resource through one bounded 8 MiB mapped staging buffer. It waits for each
staging copy to complete before reuse. All 429 uniform blocks are reserved
upfront; shader bindings use each tensor's logical range within its pool slot.
One submission executes 429 separate ordered compute passes. No CPU reference
output or intermediate is supplied to that executable. The ten pool buffers
remain alive through actual completion and a single final readback.

| Executed composed-model observation | Result |
| --- | --- |
| Hardware | NVIDIA GeForce RTX 4070 Laptop GPU, Vulkan, driver 591.86 |
| Same authored normalized RGB SHA-256 | `088ded5bdd7d05def1fdbb056f93f420a49d1327104b097f46690779fe194b80` |
| Retained CPU final depth SHA-256 | `518c5a098e453d98757197f1b8e6b53581dd956a0543fdfe35f6aa0435b54191` |
| Actual GPU final depth SHA-256 | `73f9276f7aa02f5706787b898e67fd3de65cd1dfa857e4f21b938f12fad3e6d7` |
| Output | 268,324 finite Float32 relative-inverse-depth values |
| Absolute error | Maximum 0.000011920929; mean 0.000005013256 |
| Normalized error | Maximum 0.000003775019; mean 0.000001587555 |
| Original numerical gates | Maximum 0.0002 and mean 0.00002, unchanged; no affine fit, clamp or threshold tuning |
| Retained buffer capacities | 298 buffers, 227,968,156 bytes (approximately 217.4 MiB), measured by actual buffer sizes |
| Fixed-capacity breakdown | Pool 119,539,712; persistent 98,856,704; uniforms 109,824; readback 1,073,296; staging 8,388,608; dummy bindings 12 bytes |
| One cold child-process wall observation | 7.734 seconds, including admission, device/pipelines, uploads and final result publication |
| Submission through final-map wall observation | 0.4583472 seconds; fenced completion/readback wall time, not GPU timestamps |
| Actual refusal requests | Eight indexing failures and six composed-model failures, all observed |
| Default logging channels | Info before blocking phases; original Error causes in file and stderr; empty stdout |

Composed failures cover program identity, input hash, nonfinite input, explicit
backend selection, actual resource digest and missing actual resource. The last
two use a byte-identical pinned program with isolated corrupt/missing resources;
the original model resources are unchanged. Per-node logs describe encoding,
not per-node GPU completion; the in-flight state covers the whole submitted
model while waiting for device completion. This is standalone observation,
not the product's shared job/generation contract.

These are one-input numerical and retained-capacity results. Buffer capacities
exclude driver/pipeline allocations, physical VRAM and the editor's other
resources. The timed interval excludes RGB preprocessing, letterboxing, output
remapping and transport. No sustained, thermal, photographed/video quality,
quality-switch, desktop-cross-platform or real-time result follows from it.
The static export retains the historical `static_program_only_not_executable`
capability: the separately admitted research executable does not install a
product capability or make that export consumable by msfx.

The [source and execution archive](data/depth75-composed-model-v1.zip) and
[manifest](data/depth75-composed-model-v1.json) preserve the exact Rust/WGSL,
Cargo.lock, host controls, static blueprint metadata, all three allocator
attempts, independent comparisons and actual default-level failure logs.
Every archive member was read back and hash-verified: 303,941 bytes, SHA-256
`f2479688412f9d592e73b3eaf0fa4dc73a28ccb8dfb988e2dc7c3abffbc9d704`.
Raw model/weight/input/intermediate/final tensors, executables, PDBs and
optional runtimes are excluded. Controllers retain explicit local routing;
reproduction elsewhere must record the routing change and obtain the exact
separately installed model/environment. This qualification does not replace
the separate #88 original workspace regression, which terminated with two
failures. Product integration and the remaining #75 acceptance remain open.
