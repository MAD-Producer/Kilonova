# #88 tracked-image relink publication

This increment fixes the staged image relink transaction. It does not complete
#88, video replacement, retained-window GPU cache qualification or the 3D route.

## Defect and change

**RELINK-1 / demonstrated functional defect.** The original host protocol accepted
a different, valid PNG with the same encoded length and dimensions, retained
the original calibrated camera and published the replacement. The pre-change
regression failed at the expected rejection assertion. Its log is retained;
the complete initial test source was not captured at failure time. This is not
presented as a source-bound baseline archive.

**RELINK-2 / publication admission.** A ready relink now captures immutable live
and draft state, document generation and workspace identity. Original solve-file
integrity, runtime consistency and the complete staged source SHA-256 are checked
outside the session guard before any publication. Existing source extent and
video-count checks are reused. Exact encoded identity preserves the timeline
admitted at original import; nominal FPS is never substituted for actual PTS.
The current image tests do not independently qualify the video connector.

The guard admits identical content at another path. A mismatched plate or
changed original solve fails with typed causes, a queryable failed task and no
project/revision/file publication. A changed transaction identity cannot use an
earlier proof. Cancellation and superseded documents retain their dispositions.
Untracked media still permits content replacement. No source IO is added to
ordinary frames, Present or polling that previously performed no source work.

**RELINK-3 / observations.** Info starts precede source IO; Error includes request,
session, attachment/source IDs, phase, elapsed time and bounded original causes.
Verification observations execute outside business guards. A desktop executable
test runs with MOTION_LOG unset and verifies actual file output, Error stderr,
and absence of the failure record from stdout. It exercises the worker/media
boundary, not an MCP relink tool (MCP does not expose this media operation).

## Actual acceptance

| Gate | Result | Scope |
| --- | --- | --- |
| Host relink integration | 7 passed | Bound camera, wrong equal-length source, recovery, staged mutation, original solve mutation, cancellation, superseded document, untracked replacement |
| Host tracking unit | 50 passed | Existing import/source/timeline admission |
| Host media unit | 14 passed | Existing request, retirement and media contracts |
| Host registration integration | 8 passed | Resource registration and saved history |
| FFmpeg desktop check | passed | Production host/desktop compilation |
| Desktop native worker | 1 passed | Actual nonempty spatial image capture, refused wrong source, same-source relink, undo/redo/save/reopen and byte-identical decoded output |
| Python CI tools | 110 run, 1 existing skip | Inventory and actual optimized CLI negative controls; PyYAML 6.0.3 as configured in CI |

The seven host integration tests warm the actual CPU image decoder. The desktop
test makes repeated native offscreen captures before and after the transaction,
checks a nonempty spatial image and preserves the original solve bytes. It does
not attach a native Surface or prove retained-presenter GPU cache invalidation.
No existing assertion or numerical tolerance was loosened. No panel, layout,
style, product dependency, foreign worktree or historical evidence was changed.

Source-bound reports and all retained failures are in the accompanying ZIP and
`data/tracking88-relink-closeout.json`. Host inputs remain identical to their
host acceptance report. Native desktop/host inputs remain identical to the
native acceptance report. The later CI-tool change is independently tested;
its source identity is not misrepresented as part of the earlier native build.

## CI repair and retained failures

PR #580 at 7d2e0f33 failed before Rust compilation because
`tools/tracking/calibrate_photographs.py` was absent from the explicit inventory.
The tool is now classified as optional manual external calibration. Its checks
use explicit exceptions, preserving their original limits under Python -O.
Two new CLI tests reject altered and missing original bytes before optional CV
imports; the repository's optimization-sensitive-assert guard remains intact.

The first local desktop test compilation lacked its json macro import; that
test-only import was corrected. A draft external controller also had a syntax
error before starting any child, and is retained separately. The first Python
suite used a local interpreter missing PyYAML; the complete suite subsequently
passed with the already installed CI-matching PyYAML 6.0.3 environment. These
failed invocations are not counted as passing.

An additional exact JSON equality experiment comparing repeated external
OpenCV calibration to the frozen fixture **failed**. Normal and optimized runs
both passed their unchanged measured residual limits, but their floating values
are not bit-exact repeats. In the normal run, maximum K drift was about 2.12e-10,
distortion drift 5.65e-11 and inverse-pixel drift 3.70e-9; pose-fit residuals also
varied by a few millionths of a pixel. The exact-equality assertion is retained
as failed, not relaxed or rerun for a green result. The frozen fixture and its
qualified native output were not regenerated. Bit-exact external calibration
reproducibility remains unproven; the CI inventory repair is supported by its
completed Python-suite step, not by claiming the entire experiment passed.

## Remaining work

- Qualify actual original-video relink and the separate finish-media-replacement
  route, including signed PTS, geometry and source replacement transactions
- Provide a deliberate new plate plus new solve transaction; never silently
  reuse the old solve for different source bytes
- Qualify solve/lens asset replacement and retained interactive GPU caches,
  including undo/redo/reopen; offscreen captures alone do not prove these
- Decide and verify original-plate identity at reopening after out-of-band edits;
  the current unchanged-file reopen test is narrower
- Complete the 2D/planar tracker prototype, complete maintained desktop/workspace
  regression and exact-head cross-platform CI

Verification is not a permanent filesystem lease against arbitrary external
mutation of an owned workspace after hashing. Earlier compiler/native crashes
and #82 strict performance/GPU-coverage failures remain unresolved. Android stays
excluded under the user's confirmed maintenance scope.
