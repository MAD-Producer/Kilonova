# Tracking #88: independent identity experiments

Both candidates were rejected. The product tracker remains unchanged from
`147ced69e08189b73d53760873c9ed72bc34fa36`; this report does not qualify real
tracking or complete #88. PR #582 remains a draft.

## Hypothesis and independent controls

The previous initializer discarded its descriptor reference on every explicit
manual correction. A bounded bank of the original reference and three recent
manual references could retain target identity across a reset. Two external
candidates tested distinct uses of that evidence:

- Persistent descriptors selected matches consistent with the explicit manual
  homography, then initialized adjacent-frame LK. Four matches could initialize
  a measurement, but could never publish an observation: all original 16-feature,
  12-fit-inlier, independent holdout, appearance, cycle and extrapolation checks
  remained mandatory. Automatic images were never enrolled into the bank.
- Manual photometric support rectified older manual images into the current
  explicitly corrected frame and selected plane-consistent feature locations.
  It used original photometric/cycle/agreement limits, preserved the original
  path when support was insufficient, and retained at most four manual images.

The descriptor candidate passed two new analytic controls, including an unseen
known transform after a manual reset, bounded immutable references, and absent
evidence. The photometric candidate passed a new two-motion control that selected
the moving plane and rejected stationary background features. Each separately
passed all eight existing CV controls without changing their assertions, and
the unchanged 96-frame analytic video gate. Those are useful mechanism controls,
not photographed-motion acceptance.

## Complete original-sequence measurement

Each candidate processed all 2,300 original frames in both original modes. The
source/annotation identities, initial ROI, exact timeline, initial sample and
eleven manual plane states were retained. The periodic request was compared
structurally with the preceding original request, including the explicit lost
state at index 1200. No future annotation entered automatic decisions.

| Periodic correction plane result | Automatic visible | Automatic lost | Maximum corner error (px) | RMS (px) | Above 15 px |
| --- | ---: | ---: | ---: | ---: | ---: |
| Original tracker | 915 | 1373 | 56.640030 | 5.552522 | 21 |
| Persistent descriptors | 890 | 1398 | 56.638206 | 5.621686 | 21 |
| Manual photometric support | 836 | 1452 | 57.937873 | 5.825467 | 21 |

The descriptor candidate has a negligible change in maximum error but fewer
valid observations and worse RMS. The photometric candidate also reduces
coverage and worsens the error. Neither resolves the 21 severe errors; neither
was adopted. Passing controller exit codes mean measurement finished, not that
the photographed gate passed. Source hashes were frozen and verified throughout
each controller. No existing assertion or quality threshold was weakened.

## Evidence and remaining boundary

[The receipt](data/tracking88-identity-experiments-v1.json) and
[the archive](data/tracking88-identity-experiments-v1.zip) retain both candidates,
their actual controllers, independent tests, raw test logs, synthetic summaries,
full-sequence numeric summaries and source identities. The archive also retains
the preceding rejected single-reference descriptor experiment and read-only
reference-identity feasibility study. All 60 members were read back and checked
against their recorded bytes and SHA-256 values.

Third-party video, annotations, photographs, local diagnostic captures, annotated
requests and per-frame derived payloads are excluded. Existing acquisition and
attribution records still identify the deliberately reproducible local inputs.
No dependency, UI, product return value, confidence threshold or tracker source
changed in this increment. It does not claim a new native, retained Surface,
full regression or final-head cross-platform pass.

Initialization and manual feature selection alone have not resolved the
common-mode residual failure. Further algorithm work must investigate measured
correspondence/model uncertainty, rather than repeat initializer variants or
remove failing observations to claim useful coverage. Offscreen annotation,
repeated detail, depth and distortion remain hypotheses, not established causes.
The full photographed gate, retained Surface/cache qualification, maintained
regression, #85/#86 prerequisites and the broader 3D route remain open.

## Independent recomputation of the common failures

A separate standard-library-only audit recomputed the original plane corner
errors from each retained output against the actual SHA-256-pinned annotation
archive. It checked the complete original 2,300-frame timeline and structurally
identical correction requests. It did not decode video, rerun the tracker or
modify production code. All three outputs fail on exactly the same 21 automatic
observations, rather than merely having equal failure counts. Their original
coverage, maximum-error and RMS measurements above were reproduced.

The original tracker's confidence on those errors ranges from
`0.8750063503008115` to `0.9125607289673777`, with median
`0.8978041821035365`. Other visible observations span
`0.4428490339751717` to `0.9959854803081912`. The error observations therefore
overlap the confidence range of the remaining observations. These descriptive
values do not establish a new confidence threshold or an error probability.
The next experiment must measure correspondence/model uncertainty and preserve
useful coverage; repeating an initializer or suppressing failed observations
does not establish that the plane is tracked correctly.

The [audit archive](data/tracking88-common-failure-audit-v1.zip) and
[manifest](data/tracking88-common-failure-audit-v1.json) retain the independently
authored recomputation script, aggregate measurements and input/output hashes.
Both archive members were read back and verified: 3,426 bytes, SHA-256
`2e97af1a16f9b937695f17c6102b46c06c0defc94eb72c1c823f25bfcebb36c5`.
No photographs, annotation payloads, original requests or per-frame observation
arrays are redistributed. This is a review of historical measurements, not a
new algorithm, native regression, Surface test, CI result or #88 acceptance.
