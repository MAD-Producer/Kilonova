# Tracking support diagnostics: retained negative results

These measurements use frozen prototype
`72423ec2aaf877147d20daa6827a2929bf3649b3`. They do not adopt a new tracking
algorithm or change the original 2,300-frame photographed benchmark, input ROI,
eleven scheduled manual corrections, original annotation identities or the
15-pixel corner-error gate. Benchmark payloads and derived photographs remain
external under their recorded custom public-use terms in `ATTRIBUTION.md`.

## Measured hypotheses

| Hypothesis | Independent execution | Result and limit |
| --- | --- | --- |
| Fitting-partition spatial-block deletion can expose the original bad extrapolation | Full original baseline decoded; four refits exclude each canonical ROI quadrant; all blind geometry sealed before labels | All 21 bad observations retain four measurable refits; their maximum disagreement is only 1.2462 px, below the existing 3 px consensus. This diagnostic fails to identify the real quality error |
| One-pixel image-observation uncertainty predicts the retained failure | Fixed ROI/image normalization, full-rank homography Jacobian and independent fit-partition propagation before label comparison | Small fitted uncertainty does not establish real corner accuracy; the retained original failures are not waived |
| A weak previous-transform projective prior repairs narrow support | Original eight flow controls and complete 2,300-frame video; fixed normalized ridge 0.02, no parameter search; observations sealed before labels | 922 automatic observations versus baseline 915, but the same 21 observations exceed 15 px; maximum 56.647694 px versus baseline 56.640030 px. Rejected as a quality fix |
| Partial-image support alone necessarily causes the large error | Actual source frames 0 and 1800 independently warped through known translation, rotation/scale and projective motion; six 48-frame controls, no annotation reads | All 288 observations remain visible; maximum requested-ROI corner error 0.043216 px and full-source outer-corner extrapolation 0.158967 px. This is known motion of a flat image, not proof of photographed rigidity, correct annotations or a repaired tracker |

The controlled motion always renders from the same original source image;
it does not recursively warp intermediate images. Every twelfth frame returns
to the exact original pixels, avoiding near-identity interpolation artifacts.
The known transforms are the geometric oracle. Only the existing authorized
frame-1800 correction supplies enrollment; the benchmark annotations are never
opened by that controller.

All four original evaluated corners lie outside the requested source ROI, and
the original 21 failing observations also contain failing annotated corners
outside the decoded image. This is diagnostic context only. No corner/frame is
excluded from the original metric, and annotation uncertainty or tracker
correctness is not inferred from that fact.

## Retention and remaining work

The [diagnostic archive](data/tracking88-support-diagnostics-v1.zip) and
[manifest](data/tracking88-support-diagnostics-v1.json) preserve original reports,
controllers, frozen baseline/candidate code, sealed blind observations and later
label comparisons. Original image/video/annotation payloads and derived images
are excluded. Reproduction requires the separately credited, hash-identified
inputs. Every archive entry was read back: 576,104 bytes, SHA-256
`c15065b4f24414cc8ec14bf0dec3c1fe1dc7ec5bfe7b9d90021148d019bb64ee`.

The original photographed quality gate still fails. Small model residuals,
support agreement and controlled flat-image success do not establish general
real tracking. A further method needs independent image-derived evidence for
its actual correction or failure prediction before adoption; lowering gates,
changing labels or omitting inconvenient corners would not supply that proof.
Full native/workspace/current-head CI acceptance remains separately pending.
