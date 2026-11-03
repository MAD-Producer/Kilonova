# Planar tracking matrix condition validation

Issue [#88](https://github.com/MAD-Producer/Kilonova/issues/88) remains open.
This is a functional correction to the new tracking asset validation, independent
of #82 performance acceptance. It does not change panel/UI or rendering behavior.

The previous validator divided all homography entries by their maximum magnitude,
then rejected normalized determinant magnitude at or below `1e-12`. This was not
a condition-number estimate: a perfectly invertible 16384-pixel translation
produced a tiny normalized determinant and was rejected. Large image-space
translations are ordinary tracking data, including the accepted 32768-pixel
source extent; valid homogeneous rescaling must not change that disposition.

The correction retains normalization for finite arithmetic, computes the 3x3
adjugate and evaluates `norm_inf(H) * norm_inf(adj(H)) / abs(det(H))`. The explicit
finite condition-number bound is `1e12`. Singular and nearly collapsed axes remain
refused. The original stored matrix, its homogeneous scale and samples are never
normalized, inverted, repaired or truncated. Invalid numeric conditions retain
the existing typed cause and track/sample location.

## Actual CPU evidence

The original product asset source was copied byte-for-byte into an isolated CPU
harness, with the unchanged original tests plus new regressions. The large-pixel
translation control failed as intended: **zero passed, one failed**, with the
specific valid-translation assertion. The candidate condition estimate passed
the complete direct source suite: **47 passed, zero failed, zero ignored**.
The original 44 assertions remain intact. Three new tests cover:

- 8192/16384/32768-pixel translations under tiny/large/negative homogeneous scale
- Continued refusal of nearly collapsed axes and nearly dependent rows
- Ordinary rotated, sheared and perspective pixel mappings

Evidence is retained in
`E:/Dev/aem/artifacts/pr-queue/tracking88-plane-condition-v1`, including the actual
original failure, candidate pass, exact source copies, patch applicability check
and SHA-256 receipt. A direct source harness is not host-crate integration.

## Actual serialized host validation

The [fresh host integration](tracking88-host-integration-current.md) completed
successfully: check plus exactly 44, 47 and 56 tracking tests at their respective
source stages, followed by all 169 host unit tests. The original 44-test queue
had detected a reused 35-test binary; its failure and the old zero-step queues
are retained. Updating the current crate-root timestamp forced recompilation
without changing source bytes or weakening expected counts. The earlier
`tracking88-plane-host-v1` queue is not evidence of a test run.

Actual rendered screen/plate replacement, source identity, project binding/time,
history/persistence/relink/invalidation, calibrated real footage and full #88
qualification remain pending. This correction is not evidence of a tracker or
3D camera solve. No previous failed #82 P95 result is rerun or reclassified here.
