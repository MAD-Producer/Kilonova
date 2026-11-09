# Direct physical depth placement and frozen export (#75)

Status: implementation and source-bound component controls; complete regression
and learned-model qualification remain separate gates. Desktop scope only.

## Functional change

`SavedDepthCalibration::physical` preserves declared metric or inverse-metric
camera-Z units, exact source time, model processing identity, crop/rotation,
prepared RGB digest, world pixels per metre and authored reference camera.
Physical placements contain no relative anchors, interpolation curve or
extrapolation setting. Those fields remain required and validated for relative
calibration; physical records reject them. The relative-only rebind API still
refuses physical samples. Missing relative fields, unknown fields and invalid
physical units/axis are refused on deserialization. This is a functional depth
extension; no project migration or model protocol fallback is introduced.

`Session::physical_depth_placement_command` authors an installed image admission
through the existing undoable command. Admission still requires independently
current document/source guards. The authored camera comes from the actual scene,
including when an observation camera is active. Applying/undoing/redoing authoring
invalidates old worker tickets; a fresh request must establish current ownership.

The renderer creates camera-Z geometry directly from declared physical samples,
uses the existing mesh budget and resource identity cache, and retains the exact
admitted source color. Snapshot validation, wire reconstruction, copied-source
fingerprints and independent rendering preserve the same physical interpretation.
No fake two-anchor mapping or arbitrary rescaling is used.

## Controls and retained failures

Receipts and original command logs live under
`artifacts/pr-queue/depth75-physical-session-controls-v*/`.
All work uses one serial owned controller, unchanged native toolchain and GPU,
locked/offline Cargo and E-drive outputs. Source and HEAD remain frozen through
the original terminal result. Existing tests/assertions/thresholds/ignores are
unchanged. No dependency, runtime, model, dataset, panel or visual style changed.

- v1 failed compilation: the new metadata validator returned the descriptor's
  `Result<usize>` where `Result<()>` was required. The source was corrected;
  original session 64498 / terminal 930e28 / exit 1 are retained
- v2 complete-model command failed during compilation with Windows mapping error
  1455 (insufficient committed memory), including std/test metadata failures.
  No complete-model test result is claimed. Original session 43095 / terminal 117d45 /
  exit 1 are retained. No toolchain, paging, jobs, stack or backend changes made
- v3 executes only previously unexecuted render and host targets, preserving the
  complete-model failure above. It is not a replacement green full-model result

## Limits

Controls use an independently authored analytic scalar field through real
worker/Session/renderer/export ownership. They do not qualify any learned metric
estimator, absolute/domain accuracy, continuous-video quality or default tiers.
Actual model protocol, cross-resolution and thermal gates remain unmet.
Physical aligned profile transitions still require saved-reference attachment
and frozen-placement integration; proof-level unit checks alone do not provide
that path. Physical video authoring and ray-distance placement are not delivered
by this image-only command. Prior original native shader and workspace failures
remain unresolved. Published earlier-source CI does not cover this increment.

## Source-bound results

Base `561f104708159ee275927201835a2d0940abbfcd` plus tracked diff SHA-256
6f535e25ab367a6b1df89c24ed211f8b6bb79a532ffa0bb4754f0b7fe0be05d6 and the two
new test-file hashes were unchanged through original terminal consumption.
v3 session 85312 exited 0, terminal c86f3d consumed. Complete render targets:
`depth_surface`: 51 and `scene_gpu`: 52 passed. Complete host targets: `depth_session`: 33
and `depth_video`: 26 passed. Complete host library: 295 passed, zero failed, one
original ignore, zero filtered. The original WGSL fault did not recur; this does
not resolve its cause.

The analytic physical test covers both metric forms with non-unit coefficients,
a real worker, authored versus observation camera, undo/redo, stale admission
refusal before output creation, actual GPU occlusion, wire reconstruction,
Session/source closure, independent frozen pixel equality at two camera times,
primary cancellation preservation, modified copied-source refusal and scalar
ownership release. This is one added integration test exercising two measures.

The separate model component command runs four complete selected targets:
`depth_saved_physical`, `depth_saved`, `depth_calibration`, `depth_surface`.
It passes 18 selected tests, zero failed/ignored/filtered. The new two controls
cover exact saved-identity rebind, decoded distances, incompatible metadata,
invalid physical/relative field combinations, missing relative fields and
ray-distance refusal. Its original direct exec returned 0; terminal 27f573 consumed.
This component selection does not clear the failed complete-model invocation.
