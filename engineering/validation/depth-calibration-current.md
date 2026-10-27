# Explicit depth calibration and native shared-depth checkpoint

Status: the targeted CPU/native gate passes 139 tests with zero failures or
ignores. Actual Session/FFmpeg/CPU ONNX output drives native shared depth,
with all 6,144 color and hardware-depth pixels independently verified.
Project rendering integration and cross-platform CI remain separate;
this checkpoint does not complete #74 or #87.

## Conversion and identity contract

Relative predictions retain their original measure. An uncalibrated resource
still refuses conversion to metres. Two supplied decoded-value/camera-Z anchors
select an explicit affine camera-Z or affine inverse-camera-Z curve, with an
explicit reject/extrapolate policy. Nonfinite, nonpositive, contradictory near
direction and unrepresentable results are refused, never clamped or normalized
into plausible metric values. Supplied anchor distances do not establish model
accuracy.

Immutable calibration binds to the exact source asset/revision/rational PTS,
model/hash/processing, crop/rotation/mirror/resize geometry and decoded units.
Equivalent rational PTS is accepted; stale metadata is refused before pixel
access, including invalid source samples. Missing pixels remain normal misses;
confidence does not invent a validity mask.

Projection uses the actual camera's inverse view-projection and camera-forward
Z, an explicit world-pixels/metre scale, and zero-to-one projected depth. Finite
and infinite-far perspective and orthographic cameras are supported. Crop and
rotation/mirror are applied before nearest source texel sampling. Out-of-crop,
invalid and near/far-clipped samples remain explicit invalid pixels. The bounded
owned field carries its original descriptor and exact camera identity.

## Native implementation

`ScenePass::draw_with_depth` / `encode_with_depth` accept an explicit field,
independently supplied current source sample and common scene units. The image's
full UV maps to the shared camera viewport; this is not automatic reprojection
for arbitrary transformed image planes. Source/camera/unit mismatch refuses
before mesh uniforms or output target recording.

Each encode allocates a different RG32Float depth/validity texture and viewport
uniform through the existing shared admission budget. Queue upload waits until
all source, uniform and replacement attachment admissions succeed; a later
attachment refusal cannot strand an earlier upload lease. A depth-only fullscreen
draw seeds hardware depth in the same pass before globally ordered mesh draws;
its color write mask is empty, and invalid fragments discard. Last successful
input texture appears in the resource ledger; immutable input bytes appear in
`SceneRenderStats::depth_input_bytes`. There is no implicit depth resolve for 4x.

Errors retain expected/actual asset/revision/PTS, phase, request/source and
elapsed time. The existing bounded pass observer reports first and sixtieth
failures, suppresses sustained repeats, and resets after success. Wrapped native
budget refusals preserve the original typed cause through the model adapter.
Healthy Present logging and file/Error-stderr/stdout routing remain unchanged.

## Evidence history before final execution

`depth-calibration-cpu-v4` passes all 91 model tests with zero failures or
ignores. Ten new tests independently verify calibration equations, masks, stale
identities, decoded units, extreme ranges, explicit extrapolation, both camera
projections, world units, crop/rotation/mirror and bounded allocation refusal.
All-target Clippy succeeds with exactly the same 27 relevant diagnostics as the
clean parent `9ffe5ea5`, zero new diagnostics. Strict whole-model `-D warnings`
fails on existing baseline warnings; it is not claimed green, and no allowances
or unrelated model fixes were introduced. Failed strict attempts are retained.
An initial comparison reused old-parent metadata in a shared cache; the final
comparison isolates the parent's target directory and verifies source hashes.

`depth-calibration-native-check-v2` passes compilation of the render crate and
its actual GPU test sources. The first build failure (missing zero initializer
for the new statistics field) retains its raw output and source snapshot.
Seven new native tests and one original-error boundary test are compiled;
execution has not yet established acceptance. The frozen native gate reruns
all 91 model tests, source/provenance/style checks and builds native tests before
waiting for the separate morph/skin full GPU gate. The two gates use different
targets; actual GPU execution is sequential.

The final native-v2 source adds an exact one-byte replacement-MSAA-color refusal
after admitting the new depth field, verifying complete rollback without a
pending upload. Native-v1 was deliberately stopped while waiting, before any
GPU execution, after review identified the premature upload ordering. Its
compiled source and explicit superseded receipt are retained. V2 reruns all
91 model tests, checks source/provenance/style and compiles all native tests;
actual GPU acceptance is still pending.

An external probe is prepared to connect the actual Session/FFmpeg/CPU ONNX
result to this API, verify all 6,144 color/depth pixels against independent
equations and reject a stale actual sample. Until it executes successfully,
real-inference rendering is not claimed. The external probe's final build uses
the exact product native backend features and records product-source hashes
identical to native-v2. Its illustrative anchors are not a metric-accuracy
measurement.

Project persistence, authored calibration controls, automatic current-frame
depth routing, arbitrary image-plane reprojection, mixed mesh/plane/matte/effect/
nested composition and camera selection remain open. Android is retired and
excluded. No panels, UI presentation, dependencies or third-party assets are
added, and the delivered animation movie is not rerendered. Shaders and numeric
fixtures are independently authored.

## Accepted native and actual-inference checkpoint

The final frozen `depth-calibration-native-v2` gate passes **139 tests,
0 failures, 0 ignores**: 91 model tests, all 45 native scene GPU tests and three
original typed-error boundary tests. Seven new native tests execute actual
color/depth reads in all four working precisions and both 1x/4x sample modes,
with finite/infinite perspective, orthographic projection, validity and
letterbox mapping, global transparent ordering and alpha-mask depth behavior.
Stale source/revision/PTS, camera and world units refuse before target mutation.
One-byte texture/uniform/replacement-attachment refusals, immutable pending
frames, reverse submission and abandoned reservations verify rollback and
recovery. Bounded degradation verifies threshold, suppression and success reset
at the default level. Provenance, panel-style checks and a desktop check with
both FFmpeg and AI-depth also pass. Existing assertions and thresholds remain
unchanged; compilation-only attempts above are not counted as native acceptance.

`depth-calibration-actual-v3` then executes the real Session, native FFmpeg VFR
decoder and pinned CPU ONNX model, with actual decoder PTS, crop, quarter-turn
rotation and mirror. Explicit illustrative affine camera-Z anchors produce an
immutable projected field used to occlude a real native model. An independent
reference checks **all 6,144** color and hardware-depth pixels, including
1,984 outside-crop pixels, 3,699
visible and 461 occluded inside-crop pixels.
Maximum absolute hardware-depth error is
`1.78861204181e-08` (unchanged bound `< 1e-6`).
No pixels are ignored. Supplied anchors illustrate conversion and do not prove
metric prediction accuracy.

The exact inference RGB input and raw floating output match the earlier actual
official-reference gate bit for bit; its independently produced official output
hash is verified without rerunning that reference. Previously measured normalized
errors remain maximum `7.78702453293e-07` and mean
`1.20205951624e-07`, within the original limits. This evidence
distinguishes transitive bit-exact reference verification from a new reference
inference run. Real stale-sample refusal produces Error in both the default log
file and stderr with source/revision/actual PTS, phase and elapsed time. Info start
stays file-only and stdout is empty. Product and external-probe source hashes
remain unchanged throughout. The probe is an external acceptance harness: it
does not claim GUI Engine or automatic project-render integration.

The first actual controller stopped before runtime because an earlier Windows
artifact key used backslashes. The second retained actual inference output but
the external probe overflowed its Windows executable main-thread stack while
constructing a second native Renderer. The final harness creates, uses and drops
its independent Renderer on a normal named worker, matching desktop Engine
ownership without a stack-size override. Session remains on its creating thread;
product source, GPU budgets, assertions and error bounds are unchanged. Both
failed attempts and the rebuilt harness's exact source are retained.

Evidence: [raw native/actual outputs, historical failed attempts, exact source
and independently authored numeric fixtures](data/depth-calibration-native-v2.zip),
SHA-256 `1e2b1d365827f7d97255603d4c58523df7fc2ef54c361adb24978b2bd49a918b`; 3,474,583 bytes,
223 entries, read back and verified. Runtime binaries and model
weights are excluded. Earlier CPU/compile test totals overlap this final run and
must not be added. Strict whole-model Clippy remains subject to the unchanged
27-diagnostic parent baseline documented above, with zero new diagnostics.

The parent morph/skin full workspace acceptance does not include these new
calibration sources. Maintained-main alignment, full project consumers and
cross-platform merge-ref CI must validate their own final source. Project
persistence, authored calibration, automatic current-frame routing, arbitrary
transformed image-plane reprojection, mixed project composition and camera
selection remain pending. Android is retired and excluded. No panels, dependency
or third-party asset changes are introduced, and the delivered movie is retained.


## Subsequent maintained-main combined acceptance

The calibration and receiver union now passes 496 targeted tests and all 6,144
actual inference/native pixel checks on source proven identical after rebasing
onto merged main `f6e7dbb2`. See [the exact combined acceptance and remaining
boundaries](depth-model-main-current.md). This does not claim acceptance of
later shared project consumer changes or completed cross-platform merge-ref CI.
