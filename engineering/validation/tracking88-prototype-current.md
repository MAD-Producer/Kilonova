# #88 external point/planar prototype: review boundary

This increment is stacked on PR #581, commit
`8a1dfb7f57fc1bc65683942427214b18595ac625`. It is a developer prototype;
its generated output now has the Windows native qualification below. It does not
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

## Generated-output native qualification

After publication of prototype code `330ffcac945592073a7415b41e1f88028d675398`,
`tracking88-prototype-native-v4` rebuilt the actual FFmpeg desktop executable
offline and completed with exit 0. All tracked source files were hashed before
execution and rechecked afterward. This run uses the frozen synthetic-v2
original video and the actual prototype output, not a newly authored solve.

- The real native importer verifies encoded source identity, sequential decoded
  source frames and the original timeline for both tracks / 192 observations.
  The managed solve remains byte-identical (SHA-256
  `740a6d7deeb3b618be4f6667171a54c414becabfd2cd60cbb99da5398b37eda5`).
- Native original-video capture is nonblank. Binding then samples original
  indices 0, 10, 19, 30, 40, 60 and 95. Point anchors agree with measured
  observations within 0.001 px; corner-pin values agree within 0.001 px.
- Independent polygon/marker pixel checks pass on each GPU capture: more than
  5,000 stable plane interior pixels per image, zero polygon mismatches outside
  a 2-pixel edge band, a visible point marker and centroid error below 0.75 px.
  Native checks compare measured geometry; the separate synthetic evaluator
  above compares those measurements against hidden analytic truth.
- The project runs at 125 fps while the source nominal rate is 24 fps. Each
  original millisecond PTS maps exactly to an eighth of a project frame, avoiding
  floating-point epsilon ambiguity at missing-sample boundaries. Rendering uses
  the fractional sample time; legacy PNG filename/result metadata floors the
  frame number, as implemented by the existing capture API.
- Binding undo removes both bindings and redo restores the identical project.
  Strict missing policy rejects index 22 with object/asset/track/index/visibility
  context. Explicit frame-30 manual recovery then samples and renders correctly.
  The existing Session retains the historical `renderError` string; the test
  checks that exact retained cause separately from successful current sample and
  pixels. No error clearing or product return-value changes are introduced.
- Save and read-only reopen preserve the complete project, raw solve and manual
  correction/occlusion states after the external input JSON is deleted. A fresh
  editing session also renders index 40 and passes the same GPU pixel oracle.
  Default Info import starts, Error file/stderr output and JSON-RPC stdout
  isolation are checked against the real executable transcripts.

Earlier native attempts remain in the adjacent native closeout archive. V1 used
an incomplete harness `remove` command; V2 expected an obsolete error phrase
although the actual structured context correctly refused the missing sample;
V3 incorrectly expected successful seek to clear the existing error latch. The
final harness checks the actual error contract and current render independently.
No existing repository test assertions or measurement tolerances changed.

`tracking88-prototype-native-closeout-v1.zip` and its JSON receipt preserve all
four controllers, native build/process records, raw protocol transcripts,
project state, captures and hashes. Duplicate original videos are referenced by
SHA-256 into the earlier synthetic closeout archive and the references were
verified. Duplicate exports/local logger copies are omitted in favor of named
captures and per-process log copies. This evidence qualifies Windows native
execution, not an interactive retained Surface or all supported platforms.

## Remaining acceptance

- Photographed motion acceptance: full-sequence measurement has now failed;
  see `tracking88-photographed-prototype-current.md` for coverage, visible errors,
  the reference-constraint change and its separate new native qualification
- Full native regression and exact published-head cross-platform CI
- Interactive retained Surface/cache behavior for the measured output
- Product-facing tracker workflow and the other remaining #88 delivery items

The PR remains draft while these gates are pending. Existing #580/#581 tests
qualify their own importer/replacement changes; the native qualification above
covers this prototype's measured output separately. No issue closure or complete
3D-route claim follows from this document.
