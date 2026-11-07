# Relative-depth quality transition: alignment component

This branch continues #75 after the explicit-resolution work and depends on the
shared-backend branch (#626). It does not complete #75 or enable automatic quality
switching. Alignment and pure transition admission are implemented. All fourteen
alignment controls passed on Linux, Windows and macOS Intel at commit
`5b606e0e`; these runs do not include the subsequent admission controller.
At `abf5d351`, all 30 alignment/admission/actual-Jobs controls passed in one
native Windows run (14 + 9 + 7, zero ignored, failed or filtered). The integrated
parent `5fb253a4` also passed an actual default-Info file-sink probe with four
blocked backend requests, including an original Error in file and stderr with
isolated stdout. Full-feature regression remains a separate requirement. The original native admission requires 10 GiB free.
Syntax/format checks cannot qualify the Rust behavior.

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
All fourteen controls actually passed once in each of the three completed desktop
jobs of run `42240801893`. The collector matched all 42 exact named passing test
records to the exact source head before retaining the raw logs. No existing
assertion was changed. The read-back-verified archive is
[`depth75-alignment-ci-v1.zip`](data/depth75-alignment-ci-v1.zip), 362,028 bytes,
SHA-256 `ca3aa9718dfd7a2e9f37bdddfcab07628d6d3b5d8a13c0e07b957fefe04c56a4`;
its [manifest](data/depth75-alignment-ci-v1.json) records the scope. This is not
qualification of the later admission controller or real-model/video quality.

The previously measured real 266/392/518 outputs are not globally affine
equivalents. Neither this implementation nor perfect synthetic affine controls
would qualify real transitions. Actual resource-backed execution, source-bound
real-output measurements and full regression remain required.

## Transition admission component

`depth::transition` accepts two exact, independently resolved model/hash/profile
keys. It does not interpret a backend or a larger resolution as higher quality.
Its caller supplies positive idle and minimum interactive residence durations;
there is no implicit default, system timer or sleep. Interaction immediately
requests the interactive choice. Settling requires both continuous idle and
residence. Repeated interaction resets idle, while advancing PTS invalidates old
work without itself resetting the qualified configuration's timers. Source,
revision, geometry or either model/profile changes restart both timers. Seek or
cancellation explicitly invalidates outstanding work even at an identical key.

Opaque per-controller ownership and checked, non-wrapping generations prevent
foreign tickets and A-to-B-to-A results from becoming current again. Every new
key or phase issues a new ticket; duplicate polling issues none. The caller must
check a ticket against the independent live key and exact returned resource
identity after expensive work. A successful alignment proof does not override a
later generation. Invalid keys, backwards clocks and exhausted generations are
typed errors and do not mutate prior valid state. Old results are normal
identity rejections, not logged faults.

State admission performs no inference or IO. Explicit `request`, `take_ready`
and `invalidate_jobs` helpers now connect tickets to the existing single `Jobs`
worker without creating another thread or installation path. Stale or foreign
tickets do not submit work or consume a current generation's result; mismatched
inputs are typed errors. Revocation occurs before worker cancellation. Original
worker errors remain typed sources, logged outside observation locks with bounded
context. Prepared fingerprints, actual provider and costs remain worker-owned.
The helpers add no retries and reuse Jobs' start/in-flight/cancellation behavior.
Each successful submission binds its generation to the exact worker and actual
request number. A new phase with an identical key still requires a new submission;
neither a different worker's same-number request nor an unbound later request in
the same worker can be claimed by an older ticket. Weak worker ownership prevents
allocation-address reuse without retaining scalar planes or a worker thread.

These helpers do not install unaligned depth, rewrite metadata, create cache
entries or freeze export. The caller still must qualify a capability selection,
align when required, and recheck admission afterward. No UI, automatic quality
selector or end-to-end video transition is connected by this component.

Nine new Rust controls cover exact timing boundaries, interaction chatter,
advancing frames, opaque ownership, A-to-B-to-A, explicit cancellation, independent
live/result identity, equivalent rational PTS, changed configuration, atomic
invalid-input/clock refusal, identical explicit models and a completed alignment
whose generation became stale. All nine passed in the native run at `abf5d351`; they
neither measure real-model transition quality nor replace the fourteen alignment
controls or the original full-feature regression.

Seven additional controls, all passed in that same native run, use the actual Jobs thread and a
controlled blocking backend. They check pre-submission refusal, in-flight
publication, a deliberately late success after cancellation, stale polling that
cannot consume a newer frame, explicit cancellation without replacement and an
original backend failure returned once without retry. Three of these explicitly
exercise identical-key phase changes, same-number foreign workers and later
unbound requests, checking that rejected polling leaves the ready result intact.
Release guards unblock the
fixture on assertion unwinding; five-second fixture deadlines do not change any
product timeout or existing test assertion. These controls are not model or
file-sink observability qualification.

Still pending are measured model/quality/backend capability resolution,
requested/resolved reporting, full-feature regression,
end-to-end integration of transition hysteresis, versioned caching, frozen export
policy, continuous video drift/jump controls, sustained/thermal and other-desktop-
GPU measurements. Bounds require those qualifications; these components supply
no default policy or automatic fallback.

## Source-bound native boundary evidence

The [manifest](data/depth75-native-boundaries-v1.json) and
[read-back-verified archive](data/depth75-native-boundaries-v1.zip) retain the
30 exact named passes, unchanged-source hashes, command, original stdout/stderr
and consumed native session. The separate file probe uses the actual parent
logging sink and Jobs worker: each start, processing profile and complete model
hash is read from the file before releasing the blocked backend. Three requests
succeed and the fourth returns its original typed backend error once. That error
appears in both actual file and stderr; stdout contains only one result JSON.
The file fixture was regenerated after local cache cleanup and has an explicitly
new identity. Its original failed locked-build attempt is not relabeled a pass.

The file probe qualifies parent `5fb253a4`, while the native30 run qualifies
`abf5d351`; neither result substitutes for the other, cross-platform qualification
of the latest helpers, or a full maintained-workspace pass. The full native v6
command retains ffmpeg and both ai-depth features and is still in progress.
Real-model video quality, installation and export policy remain unqualified.
