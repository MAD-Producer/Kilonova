# Relative-depth quality transition: alignment component

This branch continues #75 after the explicit-resolution work and depends on the
shared-backend branch (#626). It does not complete #75 or enable automatic quality
switching. The alignment component is implemented but has not compiled or run
yet. The original native admission requires 10 GiB free; current local space is
below that limit. Syntax/format checks cannot qualify the Rust behavior.

## Alignment contract

`motion_host::depth::alignment` estimates a positive affine map from a candidate
resource's decoded scalar values to a reference resource's decoded values. Both
must describe the same source/revision, exact rational PTS, geometry and scalar
grid, with the same relative-depth near direction. Different models/processing
profiles are allowed; their names do not establish quality or capability.
Metric/inverse-metric resources are rejected rather than relabeled.

Every acceptance limit is supplied explicitly. At most 4,096 evenly spaced
eligible pairs enter the fit. All other eligible pixels enter held-out
validation, including the final pixel; fit residuals are checked separately.
Both residual populations use the fit reference population's standard deviation
as their normalization unit. The algorithm uses constant extra memory and
bounded linear work over the existing validated resource size. Cancellation is
polled before work and at most 256 iterations apart.

An explicit minimum paired-coverage fraction also bounds support over the entire
scalar grid. A handful of valid pairs cannot establish global support. Pair-count
rejections report the population actually inspected; an unvisited holdout is not
reported as having zero valid pairs.

Validity masks remain authoritative. A requested confidence threshold requires
both confidence planes; absent confidence is not promoted to one. Statistics
retain skipped, fitted and validated counts. Constant/unstable populations,
unqualified scale, excessive residuals and cancellation are explicit typed
rejections. This is a pure bounded calculation: it does not acquire business
locks, launch workers, write logs or conceal a rejection as a missing result.
When a transition worker invokes it, that worker must publish/log its request,
generation and phase before entering the calculation and handle its rejection.

The accepted proof binds the two immutable resource-instance identities. A new
resource with identical source/model metadata cannot reuse that proof. Binding
checks both reference and candidate once; scalar access retains candidate
validity, applies its existing decode first, and reports nonfinite output as an
error. It does not rewrite raw planes, relabel the model, allocate another scalar
plane or establish a persistent cache/export identity.

## Controls and remaining work

Fourteen new Rust controls cover independent known affine coefficients, decoded
scale/offset, equivalent rational clocks, source/geometry/grid rejection,
relative/metric boundaries, constant/reversed data, explicit scale gates,
full held-out coverage beyond the fit cap, fit-population residuals, missing
confidence, masks and paired coverage, stale instances, cancellation and invalid limits.
These controls are written but unexecuted; no existing assertion was changed.

The previously measured real 266/392/518 outputs are not globally affine
equivalents. Neither this implementation nor perfect synthetic affine controls
would qualify real transitions. Actual resource-backed execution, source-bound
real-output measurements and full regression remain required.

Still pending are measured model/quality/backend capability resolution,
requested/resolved reporting, transition hysteresis, generation admission,
versioned caching, frozen export policy, continuous video drift/jump controls,
sustained/thermal and other-desktop-GPU measurements. Bounds require those
qualifications; this module supplies no default policy or automatic fallback.
