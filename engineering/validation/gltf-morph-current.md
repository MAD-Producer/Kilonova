# glTF morph checkpoint

Status: CPU, native GPU, saved-project and editor acceptance completed for this
checkpoint. Full workspace/cross-platform morph acceptance remains pending.
This document does not close #87 or claim unexecuted acceptance.

## Implemented profile

Core POSITION, NORMAL and TANGENT VEC3 float displacements are retained as owned
targets and applied before transforms. Mesh defaults, zero defaults and node
overrides remain distinct for repeated mesh instances. Target counts agree
across primitives; deltas require corresponding base attributes, matching
vertex counts, valid bounds/ranges/layouts and explicit retained/item budgets.
Other morph attribute semantics are refused rather than ignored.

Weight channels support STEP, LINEAR and time-scaled CUBICSPLINE, including
flattened per-key incoming/value/outgoing scalar groups. Normalized signed and
unsigned integer weights follow the format's defined numeric decoding. Weights
may be negative or greater than one; they are neither clamped nor normalized as
probabilities. Unanimated overrides and TRS remain authored; sampling never
rewrites project Euler keyframes or selects autoplay/looping.
Weight-only channels also preserve authored node matrices; the prohibition on
animating matrix-authored TRS does not apply to morph weights.

The independent implementation follows the public [Khronos morph target and
animation specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#morph-targets).
No dependency, external asset, third-party source or copied implementation is
added. Tests construct their own binary arrays and geometric references.

Native preparation builds immutable per-node vertex buffers for each encode.
Displacement is currently evaluated on the CPU and uploaded per encode; large
vertex/target counts need a separate performance baseline and future GPU
deformation work. No fixed frame-rate or mobile performance claim is made.
Directions are normalized before interpolation using finite magnitude
evaluation; original tangent handedness is retained. Missing normals use the
existing derivative-based flat shading and ignore tangents. A displaced zero
shading direction is explicitly refused by this rendering profile. Weights that
overflow a finite attribute also fail before recording draw commands.

Displaced primitive bounds determine transparent sorting. Source/frame vertex
capacity and the shared allocation budget both apply, with separate
`SceneRenderStats::deformation_bytes`. Mesh defaults and node overrides are
borrowed by sampled instances instead of cloned per node; default activity is
scanned once per mesh. Animated overrides own only their sampled values. CPU
regressions verify reference sharing, activity and independent values.
Pending/abandoned command lifetimes use
existing allocated encoders; later frames never queue-write earlier vertices.
Typed failures retain node/mesh/primitive and original resource rejection,
wrapped in shared-pass instance/request/source context when applicable.
The project adapter unwraps context through the original typed renderer error;
it does not turn an admitted-resource refusal into an unrelated invalid-input
error. A dedicated error-boundary test verifies the original source chain.

## Executed CPU acceptance

The exact source in `issue87-morph-cpu-v5` passes 74 tests, zero failures and zero
ignores, plus strict all-target Clippy. The source remains unchanged throughout
both commands. Seven new CPU tests cover authored defaults/overrides, pretransform
geometry and tangent handedness, three interpolation modes and endpoint clamps,
sparse displacement/frozen reimport, normalized integer streams, malformed
layouts/counts/bounds/budgets and overflow without source mutation. The seventh
checks weight animation on an authored nonidentity matrix and confirms TRS
animation on that matrix still fails explicitly.

The former blanket rejection of the `weights` animation target is replaced by
an explicit invalid-mesh assertion plus positive morph acceptance. Visibility
and skin rejection assertions remain. Two earlier owned camera/import test
initializations are simplified for strict lint without changing assertions,
values, numerical bounds or ignore annotations. Failed lint receipts v1/v2 are
retained rather than overwritten. The earlier v3 success is retained too; v4
adds the shared-reference regression and validates the final borrowed-instance
representation without changing existing assertions. V5 fixes the overbroad
matrix target refusal and adds the independent positive/negative matrix test.

## Executed native and project acceptance

Frozen source in `issue87-morph-native-v3` passes 121 tests: 74 scene CPU,
5 core model, 40 native model/render, one typed error-boundary unit and one
actual editor model-keyframe test. Zero failures and zero ignores. Provenance
and desktop FFmpeg/AI-depth checks also pass. Every command verifies unchanged
source hashes. The later parent documentation-only fast-forward to `d6789b24`
also verifies every executable source hash unchanged.

The five new standalone GPU tests and saved-project morph test execute actual
pixels in four precisions and 1x/4x samples against independently authored
static geometry. They cover distinct pending frames and node overrides, global
transparency and actual Depth32Float, mapped normal/tangent shading, admission
rollback/recovery and abandoned capacity. Saved-project acceptance reopens the
project, removes the original model asset, combines layer keyframes with
imported morph weights and seeks backward reproducibly. Existing editor
keyframe rendering is also rerun through its actual consumer.

Historical native v1 records a fixture compile typo (`0.f32`); v2 fixes only
that literal and passes all 40 native/model tests, but its new source-chain
inspection incorrectly assumes an unboxed renderer error. V3 follows the
existing mask/composition test convention and recognizes `Box<RenderError>`
as well. All budget, context and original-cause assertions remain intact;
no existing assertion, tolerance, timeout or ignore is changed to obtain green.
The complete gate is rerun on v3. Raw failed outputs, manifests and the exact
changed failed fixture/boundary sources are retained in the archive.

Evidence: [exact source, raw outputs, historical attempts and checksums](data/gltf-morph-native-v3.zip),
SHA-256 `e9caf04b7949ea9317c7456b50ff91ae99bb03456cab9a5e1754846172d5bf8b`; 2,042,909 bytes,
116 entries, all read back and verified. Weights/runtime binaries are
excluded. This is targeted final-source acceptance; the predecessor's 3,291-test
full regression is not attributed to this later morph code. Stage totals overlap
and must not be added. Cross-platform/whole-workspace morph acceptance remains
a separate gate before merge.

No panels/UI changes or repeat rendering of the delivered model movie. Android
is retired and excluded. Skinning, actual mixed project composition, calibrated
AI depth and camera-selection UI remain separate #87 work. Non-render scene
validation errors retain the existing text boundary; this checkpoint preserves
original typed renderer refusals through deformation/pass context rather than
claiming a general replacement of every scene-error transport.
