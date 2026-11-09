# Identical-pixel diagnostic of retained photographed tracking disagreement

Issue #88 remains incomplete. This is image-derived diagnostic evidence, not a
tracking algorithm, a candidate selection, an annotation correction or a quality
waiver. The original complete 2,300-frame phone sequence, 11 scheduled manual
corrections, input ROI, oracle and 15-pixel gate remain unchanged. The input's
custom public-use statement, authors/source and redistribution limits remain
recorded in `ATTRIBUTION.md`; original media, annotations and derived images or
per-frame payloads stay local.

## Why another image comparison was necessary

Earlier patch-score notes compared different feature populations. Their scores
could not establish a fair direct ordering between the predicted and annotated
homographies. This new paired observation uses exactly the same enrolled image
pixels for both hypotheses, with no feature reselection, intensity gain/bias fit,
residual trimming, alternative frame offset or oracle-driven candidate.

## Fixed measurement

The source was `d245edec44959b733a34fef635263edd7a5d00cb`. The controller read the
sealed original full-sequence output rather than rerunning or changing the
tracker. All media/request/annotation/output and source hashes were checked
before and after measurement. Original full-sequence reduction again produced
915 automatic visible, 1,373 lost, 21 above 15 pixels and maximum
56.6400304460409 pixels. Those failures are unchanged.

This is a previously diagnosed, selected 30-frame segment: 1801–1830, anchored to
the already authorized manual correction at 1800. It is not a new blind cohort
or general real-motion acceptance. The original video was decoded at those
unchanged source indices to 640x480 grayscale; no annotated still image or
synthetic render replaced the video. The original annotation convention and
source clock were retained.

The controller enrolled 12,949 integer template pixels inside the projected
original ROI `[160,180,340,210]`. For every frame it intersected the support of
both candidate mappings, retaining full three-pixel image-boundary support.
Both hypotheses then sampled the exact same template-pixel IDs and reference
intensities. The shared population ranged from 11,894 to 12,949 pixels; all 30
frames had measurements and none was hidden as zero error. Float64 bilinear
sampling was independently checked on an analytic ramp and known translation,
including an intentionally wrong mapping with exactly eight grayscale levels
of error. Boundary-support controls also passed.

Both raw grayscale and the tracker's existing fixed 5x5 sigma-1 prefilter were
measured. No smoothing value was tuned. Results below are equal-frame averages
of absolute grayscale error on the original 0–255 scale; they are not corner
error, selected inlier residuals, photometric calibration or confidence scores.

| Original diagnostic group | Frames | Raw predicted | Raw annotated H | Filtered predicted | Filtered annotated H |
| --- | ---: | ---: | ---: | ---: | ---: |
| all30_selected_frames | 30 | 6.391 | 33.513 | 3.895 | 30.091 |
| original_above15px | 21 | 6.414 | 40.230 | 3.881 | 36.534 |
| original_at_or_below15px | 9 | 6.337 | 17.838 | 3.926 | 15.056 |

The predicted mapping has smaller mean absolute error on every one of the 30
frames for both image representations, including all 21 original large-corner-
error frames. This establishes that differing sample populations alone do not
explain the image/annotation-derived homography disagreement in this segment.
Moving the measured ROI homography to the annotation-derived homography would
not improve this declared pixel registration measure.

It does not establish correct target identity, exact planar rigidity, visibility
of the external annotated corners or annotation validity. Occlusion, reflections,
nonplanarity and extrapolation can violate the shared homography/brightness
assumptions. No original corner, off-image annotation or missing frame is removed
from the original gate. This measurement does not repair its quality or coverage.

## Evidence and next boundary

The [archive](data/tracking88-common-pixel-diagnostic-v1.zip) and
[manifest](data/tracking88-common-pixel-diagnostic-v1.json) retain the authored
observation/reduction controllers and aggregate scalar evidence. Complete
per-frame diagnostic receipts and original benchmark payloads remain local.
All 3 members passed CRC, byte-length and SHA-256 readback: 10,431 bytes,
SHA-256 `19caa7419b89c91fdb5405548dbf8f9f45aa3c7f99fb4020e480c54e3226f18b`.
The original direct command completed with exit 0 in 1.469 seconds and was
consumed; source and inputs remained unchanged. This transport success means
that the observation completed, not that photographed tracking passed.

The next independent investigation must resolve what the visible ROI and the
external annotated corners physically constrain after the manual correction and
clipping. Repeating previously rejected LK windows, confidence gates or the same
ECC initializer cannot resolve this newly isolated model/identity disagreement.
The [original failed photographed acceptance](tracking88-photographed-prototype-current.md)
and [earlier negative support evidence](tracking88-support-diagnostics-current.md)
remain authoritative. No Android, panels, product source, native/Surface
regression or full current-source CI acceptance is claimed by this record.
