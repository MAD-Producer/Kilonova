# Issue 76: registered float reference-depth mixing

Status: the coordinate inverse and registered CPU depth blend are implemented.
The original blend-only execution passed all 31 selected model controls. The
shared scalar compiler and actual GPU preset execution also passed. Native
mixed-depth projection, surface construction and controlled moving occlusion
have now passed. Full integration/CI and real-footage temporal acceptance remain pending. This
record does not complete issue 76 or the 3D route.

The increment starts from maintained main
`26c1d2d2bc83c8073d85a200a7cc78cd6284d4f4`. Existing open PRs and related branches
were checked before starting; the new work is isolated from PR 606 and its
unchanged final-head CI. No panel, UI, dependency or asset changes are included.

## Implemented mapping

`DepthGeometry::depth_uv_to_source_uv` maps continuous oriented depth UV back
to full original-image UV. It undoes horizontal mirroring and clockwise
quarter-turns before applying the original crop. Texel centres use
`(index + 0.5) / size`; no integer rounding or edge clamp is introduced.

The existing validation runs first. Nonfinite inputs and invalid crop/rotation
retain explicit errors. Finite coordinates outside the depth plane return a
normal `None`, matching the existing forward mapping. This geometric inverse
does not register different cameras or presentation times and cannot establish
that two depth fields describe the same moving scene.

## Actual execution

The exact command was:

```text
cargo test -p motion-model --test depth --test depth_prepared --test depth_blend_coordinates --locked --offline -- --test-threads=1
```

All three complete selected targets passed: **12 passed, zero failed, zero
ignored, zero filtered**. The original controller exited zero after 171.703
seconds including fresh compilation; its recorded sources stayed unchanged.
Cache, target and temporary paths remained on E. Development/test debug data
and incremental compilation were disabled; no test assertion, threshold,
stack, runtime retry or existing dependency was changed.

Three new controls specify independently known source pixels for every turn
and mirror combination, check every crop corner and fractional centres, and
exercise out-of-plane/nonfinite inputs and invalid geometry. Existing depth
semantics and prepared-input controls also execute. A round trip alone is not
used as the orientation oracle.

The [archive](data/depth76-coordinate-inverse-v1.zip) and
[manifest](data/depth76-coordinate-inverse-v1.json) retain controller, original
output, source hashes and tested source snapshots. All eleven archive members
were read back and verified: 50,622 bytes, SHA-256
`0a72cc1a526e12bb6ccb642780005dbabe381068752bb2954178c9ea44a7f11e`.

## Registered blend implementation

`depth::blend::blend_depth` produces an immutable R32Float camera-Z field in
metres. Relative inputs require an explicitly bound two-anchor calibration;
metric and inverse-metric inputs retain their declared unit conversion.
Ray-distance inputs additionally require a same-grid ray-cosine plane. Both
operands must declare the same camera and world-unit basis before mixing.
Image registration alone cannot change a camera's depth basis or fix parallax.

Reference and scalar-mask registrations bind exact source, revision, rational
PTS and original image extent. Differing samples require an explicitly supplied
pixel homography, even for an identity mapping. Singular/nonfinite maps and
projective horizons crossing the output domain fail. Declared crop, rotation,
mirror and resolution are handled independently; nearest sampling outside a
registered crop remains missing instead of borrowing its border.

The operation is `mix(reference, ai, global_weight * linear_mask)`. At weight
zero the AI and mask are not read; at weight one the reference is not read.
Intermediate weights require both contributing depths to be valid. Missing
mask texels produce missing weights. Alpha and confidence are never converted
into validity, weights or invented fallback depth. Output confidence is absent.
Output values must remain positive and representable in R32Float; conversion
failure retains its output pixel index and original cause.

Output allocation is bounded by the caller's byte budget and the resource hard
limit. Metrics count actual logical AI/reference/mask sample attempts, validity
and ray-cosine plane accesses, invalid output samples and retained output payload.
They are not GPU bandwidth, allocator/RSS or elapsed-time measurements. Metadata
provenance retains both input interpretations/calibrations, all registrations,
mask geometry, global weight, camera and world-unit basis. It does not itself
persist input planes or install a render result in a Session.

The optional `scalar_presets::masked_lerp` group uses only existing Group I/O,
Multiply and scalar Lerp operations. It adds no NodeOp, resource binding or UI.
The model blend and existing compiler constant folding share the same generic
scalar arithmetic kernel; existing float compiler operator order is preserved.
Generated shaders continue using the existing generic `mix` path. Canonical
depth sampling and validity remain the owner's responsibility.

The first registered-blend execution, before the shared-kernel refactor, ran
five complete model targets: depth, prepared input, coordinate inverse, blend
and calibration. It passed **31 tests, zero failed/ignored/filtered**, in 31.422
seconds including compilation, with unchanged recorded source. Original logs
and receipt remain in `artifacts/pr-queue/depth76-registered-blend-v1`; they are
not evidence for later source changes.

Nine blend controls cover exact unused-operand endpoints, black/white/gray and
invalid mask values, calibration/direction, millimetres, inverse ray distance,
rotated/mirrored reference crop and resolution, stale sample admission, malformed
maps/weights/budgets, mismatched cameras/units and R32Float representability.
A three-frame authored foreground sequence also checks independently specified
mixed distances and costs: explicit per-frame translation avoids an old-position
ghost; deliberately wrong identity registration retains the ghost and stale
registration refuses the next PTS. This is controlled scalar data, not real video,
automatic motion estimation, temporal stabilization or an accuracy claim.

The final shared-kernel execution then passed **50 tests across nine complete
targets, zero failed/ignored/filtered**, in 451.828 seconds including fresh graph
and renderer compilation. This includes the same 31 model controls plus the
complete compiler, group and typed-default targets. Three new preset controls
validate document round trips, normal/editable compilation and actual exported
GPU pixels at five independent factor/mask combinations. The RGB byte tolerance
is two and alpha must stay exactly opaque; no threshold was relaxed. Default GPU
test features stayed enabled. The five captures and generated WGSL/GLSL are
retained; they prove scalar shader reuse, not native floating depth admission.

The [source-bound archive](data/depth76-registered-blend-v2.zip) and
[manifest](data/depth76-registered-blend-v2.json) retain the controller, unmodified
outputs, actual captures and verified tested source. Every member was read back:
729,362 bytes, SHA-256
`aa930e73aea4e98d349053d084b308fb9839bb35aeed0afeff5bf74afdbd0575`.

## Remaining delivery

- Complete final integration/CI on the published branch
- Extend temporal quality evidence beyond the controlled authored motion;
  retain explicit registration and accuracy limitations

Host authoring/history/persistence and UI controls are downstream consumers of
this model/native API; they have not been added or accepted here. The issue's
depth-mixing core does not imply those product workflows already exist.

No UI, full-workspace or real-video temporal-quality acceptance is claimed by
these increments. Android remains outside the maintained scope.

## Native metric admission and actual execution

`ProjectedDepth::from_camera_z` and `DepthSurface::from_camera_z` accept explicit
metric/inverse-metric camera-Z resources. They share the existing calibrated
geometry builders rather than inventing a relative calibration or changing the
renderer's camera/clipping/validity policies. Relative and ray-distance inputs
are rejected at admission; caller budgets still apply before vertex allocation.
Three independent controls check millimetres/inverse units against known world
positions and the orthographic clip equation, missing samples, refused measures
and bounded geometry. All four existing surface controls remain unchanged.

The final native execution passed **89 tests across eight complete targets,
zero failed/ignored/filtered**, in 97.984 seconds including compilation. This
includes 38 model controls and all 51 tests in `motion-render`'s `scene_gpu`
target, with default GPU testing enabled. The new native test runs three authored
moving frames in each of Unorm8, Unorm16, Float16 and Float32. Every hardware-depth
pixel is compared against an independent orthographic equation and source-grid
mapping with the original `1e-6` bound. Real mesh pixels must disappear behind
the moving foreground, while its old position must remain visible.

For each six-pixel blend, output retention is 30 bytes and the native projected
upload/uniform input is 64 bytes. Reference reads are exactly 6/5/4 as translation
reveals missing data; there is no hidden resize allocation. Observed CPU blend
costs were 16–36 microseconds, projection 6–13 microseconds and native submission
552–5,589 microseconds. These tiny controls are not representative throughput or
GPU duration measurements. All twelve case records and three actual native
captures are retained, separately from the scalar display captures.

The [native archive](data/depth76-native-metric-v3.zip) and
[manifest](data/depth76-native-metric-v3.json) retain exact sources, controller,
outputs, costs and images. Every member was read back: 1,969,023 bytes, SHA-256
`f4fba9edc288ffccefd71dcd7003c26c5688998501f8832ca68c668633fa8a37`.
The archive also keeps both earlier new-fixture compile failures: the inverse
unit field was incorrectly named and the pixel-index byte count needed an
explicit integer conversion. Those fixture corrections changed no existing
assertion or numerical budget. This archive is separate from the earlier tested
shared-kernel source; later source changes do not retroactively replace receipts.

The [API guide](../host-api/reference-depth-blending.md) records caller ownership,
camera/source admission, missing-data behavior and the boundary between generic
scalar ALU, depth sampling, CPU submission and actual GPU execution.

## Current mixer on retained real-video inference

A separate source-bound execution rebuilt the current model library and private
probe against product commit `4aecff0c6a7c32798e1e61cfb742f0f13c522ab7`. It replayed
the twelve consecutive NASA/ISS tour frames 450–461 already credited in
`ATTRIBUTION.md`, at their exact PTS over timescale 15,000. The 320x180 float
fields came from the retained real CPU ONNX execution of issue 74. Each input's
image hash, retained provider/sample receipt, finite plane size and original
official-reference numerical bound were checked again. This is a fresh execution
of the mixer, not fresh inference or a new validation of ONNX deployment.

One declared common calibration maps the diagnostic cohort's relative minimum
and maximum to authored 10/1 metre endpoints. Those are not measured scene
distances. The first calibrated field is held as a metric reference. Each later
frame explicitly supplies a new identity registration; no motion alignment is
inferred. Four modes (reference, half, AI and a black/gray/white linear mask)
produce **48 cases, 57,600 pixels each**, with independently evaluated arithmetic
and exact sample-cost controls. Maximum absolute numerical error was
`4.7683657911079536e-7` metres, below the predeclared `2e-6` bound. All later frames
also reject the first frame's stale registration. The probe's stdout/stderr
stayed empty. Recorded sources stayed unchanged throughout the 66.516-second
build/execution.

Actual CPU blend timings were 16,685–41,628 microseconds for these inputs; each
output retains 288,000 bytes. These debug-build observations include the mixer,
not inference, decoding or a native render. They are not a realtime guarantee.
The external diagnostic retains 48 float planes and a contact sheet of actual
video/depth/mixed fields, which was visually inspected. Same-pixel temporal
differences are recorded without a quality threshold: frozen reference gives
zero change and half mixing reduces change algebraically, which proves neither
alignment nor reduced flicker. Motion, illumination and occlusion remain mixed
into those differences; there is no physical depth or tracking ground truth.

The [replay evidence archive](data/depth76-natural-replay-v1.zip) and
[manifest](data/depth76-natural-replay-v1.json) retain current sources, private
probe/lockfile, controllers, original outputs, input provenance/hashes, every
case's costs/errors and diagnostic limits. Every archive member was read back:
565,344 bytes, SHA-256
`11d9c55c54b04b330b492b7c8581e79387239adf4528f8c66d7b4c7989c15014`.
The credited imagery and float-plane payloads remain external; their hashes and
paths are recorded, so this archive alone is not a portable-input reproduction.
It does not replace controlled native motion/ghosting proof or establish
real-video temporal-quality acceptance.
