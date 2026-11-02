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
