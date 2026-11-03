# Tracking #88: measured prototype output on an actual Surface

This is a bounded Windows qualification of the previously frozen synthetic
optical-flow output. It does not qualify photographed tracker accuracy, complete
issue #88, or complete the 3D roadmap. The draft status of #582 is retained.
Android remains excluded. No product behavior, UI, panel files, dependencies,
existing assertions or acceptance thresholds were changed by this increment.

## Source and test boundary

The prototype branch incorporates `599c35e3` and its maintained-main baseline
`e2347558`. The adjacent manifest records all tested source hashes, original
process exit, elapsed time, fixture identities, raw logs, screenshots and child
process receipts. All ZIP members were read back and checked by SHA-256.

The two independently authored MIT fixtures are frozen, byte-for-byte copies
of the synthetic-v4 source and actual measured solve. Their README records
provenance and regeneration limits. Native CI does not install OpenCV/NumPy or
rerun a numerically different tracker to manufacture a successful render.

The new ordinary GPU test is under the existing Windows/FFmpeg module, alongside
the two existing Surface controls. It creates an actual HWND and wgpu Surface,
pumps the window event loop, decodes the original video and binds the measured
point/plane tracks. `CaptureUi` reads the already committed scratch front;
it does not substitute an offscreen composition render. The UI paint list is
empty: this is not a full mouse/keyboard/interactive editor acceptance claim.

## Newly executed checks

The original Cargo invocation exited 0: **3 passed, 0 failed, 0 ignored**,
including both existing Surface wrappers and the new prototype wrapper.
Elapsed controller time: 113.468 seconds.

At 125 composition fps, original millisecond PTS map exactly to eighth frames.
The independent oracle uses the measured point and homography without calling
product projection helpers. It retains the prior native qualification's
2-pixel plane edge exclusion, more than 5000 stable interior pixels with zero
mismatches, at least 25 point pixels, and less than 0.75-pixel centroid error.
Outside those colored markers, each frame must also contain more than 5000
grayscale source pixels spanning more than 32 intensity levels. This checks
the actual native video draw, rather than merely starting a decoder worker.
Every successful Surface result must have no current render error.

The initial three-test run also passed. The final run adds these explicit
native-background and error-state checks without weakening earlier assertions;
both invocations and their different source identities are retained.
An intermediate v2 invocation has no recorded exit or test result. Its tool
handle and original processes were absent when inspected on November 3;
the original running report, raw logs and observation receipt are retained.
It is incomplete evidence, not a passing test or a diagnosed product failure.
The final v3 invocation rechecks the complete strengthened three-test group.

| Original source index | Stable plane interior | Mismatches | Point pixels |
| ---: | ---: | ---: | ---: |
| 0 | 12256 | 0 | 36 |
| 10 | 12248 | 0 | 36 |
| 19 | 12235 | 0 | 36 |
| 30 | 12206 | 0 | 36 |
| 40 | 12181 | 0 | 42 |
| 60 | 12117 | 0 | 36 |
| 95 | 11975 | 0 | 36 |

- Every paused repeat advances presentation/reuse counters without increasing
  composition renders and preserves the committed image exactly
- Strict missing observation 22 fails both seek and actual Present, retains
  the last successful Surface image and last-presented frame, and invents no
  replacement geometry
- Manual recovery at 30 produces the measured current pixels; the successful
  Surface render follows the existing error-state policy without a new reset,
  retry or product behavior change
- Binding undo/redo invalidates the front, with exact redone pixels, and save/
  reopen preserves the project, managed solve bytes and actual frame-40 image
- Default Info logs include the non-Present import start; missing-state errors
  appear in file and stderr, not stdout; healthy Present starts remain absent

## Remaining limits

The original real 2300-frame photographed quality gate remains failed, with
915 automatically visible plane observations, 1373 lost and 21 errors over
15 pixels in the unchanged baseline. Synthetic Surface success does not change
that result. Full final-source desktop/workspace regression and final-head
cross-platform CI are separate unfinished gates; historical package passes
are not new execution on this source. Project rational-rate #85 and shared
API/E2E #86 remain separate prerequisites. No issue is closed by this record.

Evidence: [manifest](data/tracking88-prototype-surface-v1.json) and
[archive](data/tracking88-prototype-surface-v1.zip).

Packaging follow-up: the fixture-local `.gitattributes` disables newline
conversion for the source and measured solve. An actual index checkout with
`core.autocrlf=true` preserves both expected hashes; see the
[checkout receipt](data/tracking88-prototype-fixture-checkout.json).
This attribute/documentation-only follow-up does not change the tested Rust
source or fixture bytes and does not claim an additional GPU test execution.
