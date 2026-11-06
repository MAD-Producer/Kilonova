# Issue 76: registered float reference-depth mixing

Status: the coordinate inverse and registered CPU depth blend are implemented.
The original blend-only execution passed all 31 selected model controls. The
shared scalar compiler and actual GPU preset execution also passed; native
mixed-depth integration and full temporal acceptance remain pending. This
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

- Connect canonical mixed camera-Z resources to existing 3D surface/clip-depth
  consumers without pretending they are relative predictions
- Complete host authoring/persistence and actual render/frozen-output admission
  with independent source/PTS guards and failure observations
- Verify native moving-sequence behavior and measured sampling/resize costs;
  retain explicit registration and temporal-quality limitations

No UI, full-workspace or real-video temporal-quality acceptance is claimed by
these increments. Android remains outside the maintained scope.
