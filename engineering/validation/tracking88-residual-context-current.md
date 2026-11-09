# Planar rejection context and rejected fit-only polishing (#88)

Status: observation improvement verified; photographed tracking remains failed.
The optional tracker now retains numeric consensus and held-out residual causes.
There is no change to geometry, confidence, visibility, loss/recovery, thresholds,
the 4,096-frame prototype admission limit or the application/UI.

## What the existing hypotheses establish

The preceding [full mouse-pad observation](tracking88-mousepad-current.md) first
lost the plane at frame14. This follow-up selected the same original frames0–14
after that failure, without annotations. All original tracker decisions remained
exact while comparing existing matrices on identical final correspondences.

| Hypothesis | Fit inliers /146 | Held-out RMS | Held-out maximum | Corner consensus maximum |
| --- | ---: | ---: | ---: | ---: |
| Original final RANSAC | 96 | 1.853996px | 5.127399px | 7.231765px |
| Existing adjacent prediction | 116 | 1.244864px | 3.638385px | 1.667678px |
| Rejected fit-only polishing | 120 | 1.262667px | 3.606538px | 2.893270px |

The existing prediction has better fitting support than the final RANSAC result,
but still fails the unchanged3px held-out maximum. An independently authored,
bounded local least-squares experiment initialized from those two existing
matrices improved fit support to120/146. It used only fitting points: no holdout
or truth affected selection. Its maximum held-out residual still exceeds3px.
This candidate is rejected and remains solely in the diagnostic archive; it is
not imported by the tracker. Improving fit consensus alone cannot justify
reviving this failed observation. No physical root cause is established.

Original diagnostic session33016 exited0, consumed chunk`9aee0c`; experimental
session45398 exited0, consumed chunk`2843ab`. Both preserved all15 original
observations and source/input hashes. Exit0 means diagnostic completion, not
tracking quality. These selected measurements do not replace either original
complete photographed failure or establish general robustness.

## Default-level numeric causes

Previously the rejection identified only “too few inliers” or “held-out residual
exceeds limit”. The same checks now retain bounded measured counts, ratio,
thresholds, RMS and maximum in the original failure cause. The existing
per-track counter still emits first Debug and occurrence60 Error once, with the
same reset rules. Error reaches both file and stderr; stdout stays empty.
There is no additional healthy-frame logging, retry, fallback or model selection.

The original positive acceptance comparisons are preserved, including equality
and rejection of non-finite residuals. New real numeric controls exercise an
actual RANSAC12/20 rejection, independent RMS and maximum rejection, inclusive
boundary acceptance and non-finite rejection. No existing assertion changed.

## Actual complete equivalence and output verification

Session7653 ran at HEAD`87e648ff` with the explicitly recorded dirty source
hashes; it exited0, terminal chunk`311745` was consumed before editing resumed.
It ran all10 optional real-CV tests,10 ordinary tracker admission tests and2
calibration admission tests. All22 passed. These are Python/tool controls, not
full native/workspace regression or current-head cross-platform acceptance.

Both original6,946-frame core modes were then decoded completely with the new
context. Every source index, PTS, geometry, confidence, visibility and manual
identity equals its sealed original observation. Only bounded cause extensions
differ:6,932 samples without corrections and3,558 with scheduled corrections.
All original losses and corner-error failures therefore remain unchanged.

With`MOTION_LOG` absent, Info starts are present, Debug is absent, and all Error
records match file versus stderr exactly. The uncorrected plane emits exactly
one occurrence60 Error at source index73, including
`inliers=96 fit=146 ratio=0.657534 minimum_count=12 minimum_ratio=0.7`.
There are2 total Error records without corrections and49 with the original
scheduled corrections. Both child stdout files have zero bytes. This verifies
the actual failure path, suppression and output destinations at default level.
Neither core mode produces an admitted tracking asset or waives the CLI cap.

## Retained evidence and remaining work

The [archive](data/tracking88-residual-context-v1.zip) and
[manifest](data/tracking88-residual-context-v1.json) retain authored controllers,
the rejected candidate, numeric summaries, source snapshots, consumed-session
records, complete control logs and original file/stderr observations. All31
members passed CRC, byte-length and SHA-256 readback. Archive size33,016 bytes;
SHA-256`8c0b71c98fb63745e7d865c22486a44854918e30ed5f99b5d06e09fce580bffd`.
Original media, annotations, images, requests and full per-frame streams remain
local under the existing attribution; they are not redistributed.

Issue #88 remains incomplete. Correspondence/model quality, useful photographed
coverage, original phone failures and full native/current-head CI acceptance
remain separate unresolved boundaries. No Android, panels, dependency, dataset,
runtime acquisition or foreign process change is included.
