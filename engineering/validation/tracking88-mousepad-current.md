# Independent original mouse-pad sequence and selected first loss

Issue #88 remains incomplete. This first complete core measurement of the
already acquired, attributed mouse-pad video adds independent evidence. It does
not replace the original phone failure, qualify useful coverage or change gates.

## Input, blind outputs and prototype refusal

The unchanged input contains6,946 original640x480 frames at100/1fps, with PTS
0–6945 in a1/100 timebase. Annotations are named`img_00100.txt`–`img_07045.txt`.
Source row/column MATLAB coordinates are converted to zero-based x/y, matching
the phone convention. Source, annotation and permission hashes are retained;
payloads stay local under the custom public-use statement in`ATTRIBUTION.md`.

Before any run the ROI was fixed at`[200,140,340,260]` from the archived first
frame. Two modes were declared: no corrections and explicit manual corrections
every200 frames. Only initial and scheduled manual coordinates were read before
tracking. Both complete streams were sealed before automatic-frame annotations
entered the separate reduction. There was no interval selection, label-driven
automatic input, parameter search, hidden retry or omitted missing observation.

The actual CLI returned1 with its original cause:
`prototype frame count exceeds 4096; source is not truncated`. Default file and
Error stderr contain that cause; stdout is empty. Its4,096-frame contract stays
unchanged. The full core diagnostic bypasses only the prototype entry, producing
diagnostic JSONL rather than an admitted`.mstrack` asset. The CLI remains
refused. Core measurements do not prove native import/export or whole acceptance.

Source was`d21b9df82f5f3c7b4a35af2bfd5d4e91b7928231`. Existing tracker code,
thresholds, flow options, prefilter and loss/recovery behavior were reused
unchanged. One owned controller ran the refusal followed by both full core
streams. Source and inputs remained unchanged until original session3421
exited0 and terminal chunk`3b302c` was consumed. No owned heavy run overlapped.

## Complete results

Initial enrollment is excluded from automatic counts. Corrected mode has34
manual samples per track. Every row accounts for all6,946 frames as initial,
automatic visible, automatic lost and manual. Missing is never zero error.

| Mode / track | Automatic visible | Automatic lost | Manual | Above15px | Maximum error |
| --- | ---: | ---: | ---: | ---: | ---: |
| No corrections / point | 96 | 6,849 | 0 | 0 | 3.685786px |
| No corrections / plane | 13 | 6,932 | 0 | 0 | 2.756288px |
| Every200 / point | 3,049 | 3,862 | 34 | 5 | 16.205062px |
| Every200 / plane | 536 | 6,375 | 34 | 0 | 6.450440px |

Both modes consumed all original decode frames, retaining indices and exact
PTS. Corrected plane error below15px does not establish useful coverage:6,375
automatic samples remain lost. The point has five explicit quality failures.
Controller exit0 means measurement completed, not that tracking passed.

## Observe the first loss without changing it

The uncorrected plane first becomes lost at index14. A separate diagnostic
selected indices0–14 after that failure. It wrapped existing homography calls,
returned their original matrices/masks, and independently measured residuals.
All15 plane observations, confidence values and reasons exactly match the sealed
full stream. No annotation was opened. Session31840 exited0; terminal
chunk`c7db5e` was consumed before source editing resumed.

At the failed final fit,146 fitting and46 held-out points remain. The original
RANSAC mask marks96 fitting inliers; independent evaluation of its returned
matrix at the unchanged1.5px threshold also marks96. Actual ratio96/146 =
0.657534 is below the unchanged0.7 gate. This rejects a stale-mask explanation
for this selected first loss. It does not establish why these correspondences
fail a single homography, or prove nonplanarity, distortion, flow error, target
identity or annotation error. It does not justify weakening the residual gate.

## Evidence and remaining boundary

The [archive](data/tracking88-mousepad-diagnostics-v1.zip) and
[manifest](data/tracking88-mousepad-diagnostics-v1.json) retain authored
controllers, aggregate reduction, exact tracker source, original file/console
diagnostics and consumed-session records. Archive size31,258 bytes; SHA-256
`53d6f7aa73cd3da8a90be45887593338af1bfe360c359b8dab059f4439c1d7ee`.
Every member passed CRC, byte-length and SHA-256 readback. Media, annotations,
derived images, requests, complete per-frame observations/PTS and the copied
source page are excluded. Complete private observations and receipts stay local.

The [phone quality failure](tracking88-photographed-prototype-current.md) and
[identical-pixel disagreement](tracking88-common-pixel-current.md) remain
authoritative. This independent sequence exposes a coverage limit even where
admitted plane observations have small corner error. Further algorithm work
needs an independently justified correspondence/model improvement, then both
complete original sequences. No tracker, Android, panels, dependency or default
application behavior changed. Native/full-regression/current-head CI acceptance
remains separate.

The subsequent [rejection-context follow-up](tracking88-residual-context-current.md)
shows that better fit support alone still fails the original held-out maximum.
The fit-only polishing candidate was rejected. Numeric failure causes now expose
the measured gates; both full core modes preserve every original observation
apart from the explicitly extended cause text.
