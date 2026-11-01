# #88 external point/planar prototype: review boundary

This increment is stacked on PR #581, commit
`8a1dfb7f57fc1bc65683942427214b18595ac625`. It is a developer prototype,
with native product integration and real-motion acceptance pending. It does not
complete #88. Android is excluded. No panels, product dependencies or UI changed.

## Delivered observations and behavior

- **PROTO-1 / original-source admission:** request SHA-256 is checked before
  starting external tools or importing CV packages. Explicit stream selection,
  raw dimensions, exact signed PTS, strict ordering and decoded frame count are
  retained. Original bytes are rechecked before publishing tracking JSON.
- **PROTO-2 / automatic measurements:** forward/backward point flow; planar
  RANSAC with held-out correspondences excluded from fitting. Invalid or absent
  observations stay missing. Manual corrections are explicit and retained.
- **PROTO-3 / failure visibility:** Info starts and phase/PID reports precede
  external blocking work. Errors go to file and stderr with stdout isolated.
  Per-track consecutive automatic loss is bounded, first Debug/60th Error once,
  and reset by recovery/manual states. Raw child diagnostics are drained with
  bounded retention. Normal authored occlusion does not count as a fault.
- **PROTO-4 / scope:** optional external tool versions and executable identities
  are recorded. No automatic downloads, UI wiring, product retries, worker
  restarts, or camera-solver claims are introduced.

## Executed evidence

The frozen `tracking88-prototype-synthetic-v2` controller completed with exit 0.
It captured source hashes before execution and verified them afterward. Its
encoded authored video has 96 frames at 256 x 192. Analytic truth is used by
the evaluator and explicit manual corrections, not automatic tracking.

| Track | Automatic measured frames | Maximum error (pixels) | RMS (pixels) | Fixed maximum budget |
| --- | ---: | ---: | ---: | ---: |
| Point | 84 | 0.1262063894 | 0.0629499734 | 0.75 |
| Plane, independent probes | 84 | 0.1552570762 | 0.0687947221 | 1.0 |

Occlusion is authored at frames 20-24; tracks remain lost through frame 29,
with explicit manual recovery at frame 30. Every other automatic frame must
produce a visible result. A separate flat 80-frame clip verifies persistent
loss, exactly one Error at occurrence 60 in both file and stderr, and empty
stdout. The original motion file SHA-256 is
`0046b8a42839c23b02f0c6c1558cf77c0e7ec580a4d539719722fee65c018d1f`.

The earlier synthetic-v1 failure is retained: point appearance error was
12.09410477 against a limit of 12 despite a 0.00453256-pixel forward/backward
residual. Filtering both images before flow corrected this measurement problem;
the fixture, coverage requirements and acceptance limits were not weakened.

Publication validation ran `python -m unittest discover -s tools -p
'test_ci_*.py' -v`: 118 discovered, 117 passed and one skipped, including eight new admission/process
tests. These exercise actual CLI rejection in normal/optimized Python, original
PTS, missing/correction contracts, threshold/reset, child cancellation and
bounded stderr draining. This is not a native Rust/GPU test claim.
The skipped existing test requires `MOTION_EFFECT_VALIDATOR` for the native
effect schema/WGSL loader; that validator was not configured in this tools run.

The adjacent closeout ZIP preserves both synthetic runs (including the failed
run), their input video bytes, requests, truth, tracking output, source captures,
raw logs and publication test log. The adjacent JSON records archive SHA-256 and
member hashes. Reproduction commands and request schema are in
`tools/tracking/README.md`. Source hashes identify the tested files independently
of later documentation/commit metadata.

## Remaining acceptance

- Native import of this generated output against its original video, binding,
  actual rendering, undo/redo, save and reopen
- Photographed motion qualification; authored analytic texture is not real footage
- Full native regression and exact published-head cross-platform CI
- Product-facing tracker workflow and the other remaining #88 delivery items

The PR remains draft while these gates are pending. Existing #580/#581 tests
qualify their own importer/replacement changes, not this prototype's generated
output. No issue closure or complete 3D-route claim follows from this document.
