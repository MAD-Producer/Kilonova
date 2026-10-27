# Maintained-main calibration and model receiver checkpoint

Status: accepted targeted combined execution; cross-platform merge-ref CI and
the remaining project consumers are separate. This checkpoint does not close
#74 or #87. Android is retired and excluded.

## Exact source and execution

The calibration and model receiver repairs were combined, then rebased onto
maintained main `72d23d74` before execution. All own patch Git blobs remained
identical. A markdown checkout line-ending change is recorded separately from
product source; it is not presented as a functional change.

`depth-model-main-v1` executes **496 passed, zero failed, zero ignored**:
all 91 model tests and 405 native render tests across 15 targets. Native coverage
includes the renderer library, scene GPU, model layers, retained layer rasters,
exposure frame/allocation/accumulation/SDR/scenes, nested exposure, masks and
their error boundaries, scene effects, typed snapshot execution and ordinary
effects. Both cold model receiver regressions, their independent linear
premultiplied shutter reference and all calibrated shared-depth tests pass.
Provenance, panel-style consumers and desktop compilation with FFmpeg and
AI-depth also pass. Earlier totals overlap and must not be added.

`depth-calibration-actual-probe-build-v6` rebuilds the unchanged external harness
against this exact combined source. `depth-calibration-actual-v4` executes real
Session, native FFmpeg VFR decode and CPU ONNX inference, then checks all
**6,144** color and hardware-depth pixels. Maximum absolute depth error is
`1.7886120418086904e-08`, below the unchanged `< 1e-6` bound. RGB and raw inference
output remain bit-exact to the prior independently verified official-reference
gate. Illustrative supplied anchors do not establish metric prediction accuracy.
Default Info start, actual stale-sample Error in file and stderr, and empty
stdout are verified again. The external harness does not claim GUI Engine or
automatic project-render integration.

After all required PR #526 checks succeeded, that own PR was squash-merged as
`f6e7dbb21b809894062c3fcf7b9be9fd2845362d` with its actual Chinese title verified.
The follow-up was then rebased onto that main, retaining just the calibration
and receiver repair commits at `76006cff`. Every recorded execution input and
the complete recorded source manifest remain byte-identical. This is a proven
source-equivalence transfer, not a second execution or a full-workspace claim.

## Evidence and boundaries

[Raw combined outputs, actual input/output, source snapshots, rebuilt harness
and both main-alignment receipts](data/depth-model-main-v1.zip): SHA-256
`d68b7c8e73b60fb8b4de4adc689eb17994c91d2ae9fd3c24ce7000e25e870279`,
2,059,683 bytes, 154 entries, independently read back and verified. Executables,
shared libraries, model weights and Python bytecode are excluded. Historical
failed attempts remain in the individual calibration and receiver archives;
this archive records their later combined accepted execution.

No panels, styles, dependencies or third-party assets are changed. No delivered
movie is rerendered. Assertions, numeric bounds, budgets and timeouts are not
relaxed. Strict whole-model Clippy retains the previously recorded unchanged
27-diagnostic baseline; it is not claimed green.

Later work on a project model shared-depth consumer changes renderer code and
requires its own actual acceptance. It cannot inherit this checkpoint merely
by being developed while CI runs. Mixed model/plane/matte/effect/nested depth
composition, persisted authored calibration, automatic current-frame routing,
arbitrary image-plane reprojection and imported camera selection remain open.


The later [final combined checkpoint](depth-project-final-current.md) rebuilds
the real calibrated inference harness after the project shared consumer and
records its exact final source and scope. Earlier gates are not added together.
