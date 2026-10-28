# Integrated camera and depth authoring regression

## Accepted local source

The frozen affected-suite regression passes **879 Rust tests, 0 failed, 0
ignored**, plus **13 Python workflow tests**. Counts are scene 85 across nine
targets, model 96 across 25, core 88 across 17, native renderer 450 across 18,
and host 160 across ten. Host depth-disabled compilation and desktop
FFmpeg/AI-depth compilation, provenance and panel-style checks also pass.
Earlier checkpoint counts overlap and must not be added to these totals.

The tested main baseline is `2af21061ec2a8f8ebf465dd083d29ba5ea4099e1`
and implementation head is `a451d80de80b7cc7473330d645f4a5bcdfd0a36f`.
This includes the accepted camera ancestry source from PR535, the persisted
calibration/current-input stage and frozen video camera-source ownership, plus
the main CI/acceptance updates. The ancestry-aware preparation callback retains
both the current camera resolver and candidate depth-calibration validation;
neither was dropped during conflict resolution. Every step verifies source
hashes. The later completion amendment changes documents/evidence only.

## Covered behavior

The gate reruns actual GPU model, imported camera, parent, alpha, image, mask,
effect, nested composition, resource-refusal and immutable upload regression
suites. New camera ancestry, actual host drag/align, persisted calibration,
actual prepared RGB hash and frozen-reader CPU/GPU tests all execute.

The frozen reader owns the snapshot's immutable catalog and independent source
clock after live camera edits, session close and source-file removal. Native
pixels at frames 0 and 30 match live sampling at all four precisions, while
animation visibly changes the mesh output. The final reader release drops its
last retained parsed source. Snapshot wire messages, reader capacities and
sequence/failure recovery behavior remain unchanged.

Saved calibration is admitted against actual prepared RGB, exact source time,
model/geometry/interpretation and the separately guarded current session
result. Commands use ordinary history and rollback. Fresh requests after
commit/reopen rebuild a live revision-bound calibration; changed pixels with
unchanged predictions are refused. No old runtime revision guard was removed.

Two preliminary combined invocations stopped before Rust tests because the
local Python invocation lacked its existing pinned PyYAML path, then the tools
discovery path. The final invocation uses the repository's documented unittest
discovery and the already installed dependency on E. No test, assertion,
threshold, budget, timeout or skip was changed to address those environment
errors. Their raw loader output is retained rather than counted as coverage.

## Limits and remaining work

These are source-bound affected suites, not complete workspace, cross-platform
or final #87 acceptance. PR535 retains its separate cross-platform CI. Native
frozen-frame output is covered; encoded video/container output still needs
end-to-end acceptance. Persisted calibration remains authoring/admission data:
ordinary preview/capture does not yet consume it as native geometry depth.

The actual current-frame render bridge, source-camera and transformed-plane
reprojection, multiple calibrated layers and units, native seed budget/error
recovery, actual reprojection/DOF output, complex effect/mask/matte/nested depth,
intersecting transparency and final maintained-desktop acceptance remain open.
Neither #74 nor #87 closes. Android is retired and excluded. No UI or new
third-party asset changes are included.

[Exact-source evidence](data/depth-camera-integrated-v3.zip), SHA-256
`49be0c51e4cde4f2746d009277e4054fd870eaeb30e2edbccf5d064916d4d509`, 1863565 bytes. The archive retains source,
the patch against the tested main, commands, controllers, raw native output,
exit receipts and the preliminary invocation failures. Every archive entry was
read back and SHA-256 verified.
