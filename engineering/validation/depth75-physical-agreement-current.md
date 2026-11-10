# Physical depth agreement before quality switching

Status: unit-contract implementation and analytic controls passed. No learned
metric model, physical Session binding or production quality tier is qualified.

Relative depth retains the existing explicit affine fitter and its original
gates. Physical depth takes a separate path: `Alignment::verify_metric` derives
its sole coefficient from the two declared units and fixes the offset at zero.
It never estimates scale or offset from the observations. A model's multiplicative
or additive physical bias must therefore fail the residual gate rather than be
hidden by alignment.

Metric values require the same depth axis and physical representation. Camera-Z
and ray distance are not interchanged. Metric and inverse-metric resources are
not mixed, nor can relative values be relabeled physical. Both metric forms
convert candidate decoded values to reference units with
`candidate_unit / reference_unit`; raw resources, metadata and model identity
remain untouched. `sample_measure()` exposes the interpretation of mapped values,
separately from the original candidate descriptor.

Worker transition admission and frozen proof reconstruction dispatch by the
declared reference measure. Existing source/PTS/geometry/grid binding, confidence,
coverage, fit/held-out partition, normalization, coefficient bounds, cancellation
and immutable-instance checks remain mandatory. Physical comparisons use the
caller's existing explicit normalized residual limits and require nondegenerate
reference variation. They are consistency checks, not absolute accuracy or domain
qualification; no new default acceptance threshold is supplied.

Alignment policy version is now 2. Frozen records must name the current policy;
version 1 is rejected, without migration or silently trusting old coefficients.
This follows the active iteration's explicit lack of legacy-runtime compatibility
requirements. Existing relative calibration and frozen Session fixtures retain
their assertions and use the current version constant.

## Source-bound verification

One locked/offline serial controller ran on base
`687cfda6835d575156b461e8a52b3103d1d2b9ca` with tracked diff SHA-256
`9aa39a0a6a31d6e730c68655f464f284315b8ea972deafdcd69aad537eed1be2`.
No source or HEAD changed before its original terminal result was consumed.
Private records: `artifacts/pr-queue/depth75-physical-agreement-controls-v1/`.
Original session 65142 exited zero; terminal chunk 92177a was consumed.

- Complete `depth_alignment`: 17 passed, zero failed/ignored/filtered
- Complete `depth_transition`: 9 passed, zero failed/ignored/filtered
- Complete `depth_transition_jobs`: 7 passed, zero failed/ignored/filtered
- Complete `depth_worker_alignment`: 8 passed, zero failed/ignored/filtered
- Complete host library: 295 passed, zero failed, one original ignore, no filtering

Five added controls verify metric and inverse-metric unit conversion, unchanged
raw descriptors and physical distances, rejection of scale/offset bias, incompatible
axes/forms/relative units, coefficient bounds, coverage, independent heldouts,
missing confidence, cancellation and foreign-instance proofs. Real Jobs/controller
controls admit declared unit conversion and preserve a biased result's original
failure without returning raw output. Frozen reconstruction checks the original
planes again, rejects bias and rejects the old policy version. These are analytic
resource/worker controls, not real learned-model or domain evidence.

No existing assertion, threshold, ignore, backend, compiler, stack or deadline
changed. No dependency, model, runtime or dataset was acquired; no panels or UI
behavior changed. The published 687cfda6 CI is separate and does not cover this
later increment. Original native failures, model/cross-resolution/thermal gates,
physical estimator protocol and complete Session/export integration remain open.

Subsequent work: [direct physical Session/export placement](depth75-physical-session-current.md)
now has analytic live/frozen rendering controls. Physical aligned-transition
placement, actual learned-model protocol and qualification still remain open.
The new component evidence preserves the original complete-model native failure.

Follow-up: [physical aligned Session/export placement](depth75-physical-transition-current.md)
now has analytic metric/inverse-metric transition, actual rendering and frozen
reconstruction controls. This later implementation supersedes only the earlier
physical-alignment integration gap, not the unmet learned-model, complete-model
regression or production-quality gates.
