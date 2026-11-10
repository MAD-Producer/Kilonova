# Selected first-loss correspondence phases (#88)

This post-failure diagnostic measures the original mouse-pad plane loss at
source index14. It is neither a new blind cohort nor an algorithm improvement.
The complete original6946-frame coverage/error failures remain authoritative.

Source `cd4456135a169018e343fd79c5e85f09fd04c3ca`, existing CV runtime, input
hashes, feature selection, random seed, flow/fitting options and rejection gates
were frozen. Wrappers copied the original function results without modifying
their arguments or returns. All15 original plane observations and confidence
values matched the sealed full stream exactly; the rejection text only includes
the previously added numeric context. No parameters or candidates were searched.

## Reference enters after sealing

The complete selected observations and final correspondence arrays were written
and hashed before opening two corner annotations, at indices0 and14. The sealed
private arrays retain the same192 surviving correspondences, split into146 fit
and46 held-out points. The four annotated corners define a reference homography
from the source frame to index14, using the previously declared row/column,
one-based-to-zero-based conversion. They do not enter the tracker, feature
selection, fitting, validation gates or candidate selection.

This is an annotation-derived four-corner reference, with unknown annotation,
lens and physical-planarity uncertainty. Its implied position at an interior
feature is not independent measured subpixel ground truth. Values below are
disagreement with that reference; they are not new admission thresholds.

| Original phase, same surviving points | Fit RMS | Held-out RMS | Held-out maximum |
| --- | ---: | ---: | ---: |
| Adjacent-frame optical-flow positions | 1.534027px | 1.662056px | 3.525615px |
| Reference-anchored positions | 1.584901px | 1.684811px | 3.891933px |
| Prediction homography evaluated at reference features | 0.952483px | 0.956666px | 1.368914px |
| Final homography evaluated at reference features | 2.049078px | 2.018125px | 5.068345px |

The actual anchor step has RMS displacement0.124963px, median0.068226px and
maximum0.524671px over all192 surviving points. Adjacent positions already
disagree with the annotation-derived reference before anchoring. These selected
values do not support attributing the entire failure to reference resampling.
They do not prove that deleting anchoring would fix the original sequences.

At the four annotated corners the prediction's maximum disagreement is
1.953007px; the final fit's is10.750808px. The selected final fit amplifies the
correspondence differences when extrapolated beyond the feature support. This
does not identify whether the original cause is flow error, feature geometry,
estimator instability, lens/plane mismatch or annotation uncertainty.

The actual tracker still refuses96/146 fitting inliers, ratio0.657534 below0.7.
The [earlier identical-correspondence investigation](tracking88-residual-context-current.md)
also found that the prediction and fit-only polished candidate fail the
unchanged held-out maximum gate. A model's smaller disagreement with labels
opened after sealing cannot override that independent failure. No alternative
model is adopted, gate weakened, feature removed using labels, or lost sample
converted into visible output.

## Evidence

Original controller session27677 exited0; terminal chunk `836476` was consumed
after checking all tracked hashes, HEAD, inputs and blind seals. Only then was
the source freeze released. The result means that this selected measurement
completed, not that tracking passed. One owned controller ran, with no new
dependencies, models, datasets, foreign-process access or environment workaround.

The [archive](data/tracking88-flow-phases-v1.zip) and
[manifest](data/tracking88-flow-phases-v1.json) retain the authored controller,
original aggregate receipt, complete diagnostic/decode logs, terminal consumer
and collector. All7 members passed complete CRC and byte-for-byte readback.
Archive size299,276 bytes, SHA-256:
`3c883c6569bd7f3b3ca272ba01bf40949acefe46cd8acc08a8dc64b8d1fecc65`.
The collector exited0, consumed chunk `af738b`, without a running session.
Media, corner annotations, images and correspondence/observation arrays stay
private; their seals are retained. Original prototype/native, quality,
full-regression and current-CI boundaries remain separate. Issue #88 stays open.

The subsequent [single fixed MAGSAC check](tracking88-magsac-fixed-fit-current.md)
uses the same sealed fitting points without labels. It still fails the original
held-out maximum gate and is not adopted. No estimator/parameter sweep follows.
