# Current desktop AI-depth research acceptance (#75)

Status: incomplete. This document separates implementation, observed results and
unmet acceptance. No single component pass, old-source CI result or diagnostic
archive establishes completion of #75 or the complete 3D roadmap.

The maintained delivery scope is desktop. Android is retired and its device
qualification was explicitly waived. Old project/runtime compatibility is not
a delivery requirement during this iteration. No external learned-model weights
are added to the product or evidence archives.

## Requirement-by-requirement state

| Issue requirement | Current evidence | Remaining acceptance |
| --- | --- | --- |
| Explicitly licensed candidate; operators, shapes, dtype, weights, activation and workgroup/storage limits | The attributed Depth Anything V2 Small revision and export are pinned. All 598 nodes and 23 operator kinds are inventoried; the fixed-shape Float32 plan has no unknown shapes. Original weights and resource estimates are documented in [candidate research](depth75-wgsl-candidate-current.md). | The candidate remains an external research input. Its name and numerical equivalence do not qualify a production domain, quality tier or default distribution. |
| Same-weight WGSL/ONNX comparison and measured full-chain cost/thermal behavior | Per-operator/end-to-end controls and the original 32-source-coordinate comparison exist. The new retained Rust-remap route completed its first fixed one-service run:25 complete cohorts,800 requests,1,835.03 seconds, all full Float32 outputs byte-identical to their sealed cold references. | Peak temperature90°C and hardware/software thermal flags remain observed. Original cold/Python-path crashes remain failures. This is neither realtime nor thermal, domain or cross-device qualification. |
| Shared DepthResource/job interface; quality determined by model/resolution rather than backend name | ONNX and WGSL implement the shared Backend/Jobs route. Measured jobs, persistent cache, Session installation, source preparation, calibration and immutable export are integrated. Model/geometry and actual execution identities are independently checked. | These are explicit host APIs and examples. The desktop application does not configure an automatically qualified inference route merely because these APIs exist. |
| Measured fast/standard/high selection, requested/resolved visibility and explicit precomputed fallback | Capability policy retains requested limits, actual profiles, refusal reasons and admitted bindings. Missing/changed execution identity refuses before cache or inference. Missing qualification returns PrecomputedRequired rather than inventing inference success. | No actual learned candidate has complete independent domain-quality, conservative memory and sustained qualification for automatic selection. Controlled policy fixtures cannot supply that qualification. |
| Aligned, versioned, hysteretic and generation-checked interactive/settled transition; explicit frozen export strategy | Worker/Session transitions, stale-generation and runtime refusals, retained input ownership, calibration and frozen restore have controls. Same-weight CPU518/WGSL518 alignment accepted all 32 original coordinates. | CPU266 and CPU392 each failed all 32 original cross-resolution fits. The unchanged research ceilings are retained. An accepted actual continuous-video cross-quality transition is not established by same-quality backend equivalence or synthetic fixtures. |
| Versioned compute/weight resource plan and capability negotiation with non-inference fallback | The [resource/consumer boundary](depth75-resource-negotiation-current.md) now records the strict version1 research plan, separate resources, exact execution admission and retained-command protocol. Normal image-effect packages do not accept model/weight resources; frozen depth can be restored/rendered after the worker exits. Nine original admission controls are verified in the full log. | This is the explicit research route and its planned boundary, not a generic distributed model-package installer or default application configuration. Production distribution and qualification remain pending. Retired Android compatibility is outside scope. |

## Current implementation and measurements

`Wgsl::new_retained_rust_remap(Config)` is an explicit changed mode. It preserves
Python RGB preparation and attestation, retains one owned native GPU service,
and performs final crop/Float32 bilinear remapping in Rust. Host executable,
platform and remapping procedure identity distinguish its cached results from
the cold and Python-remap routes. It does not implicitly restart a failed
service or select another backend.

The [32-input observation](depth75-rust-remap-current.md) retained original
ordering, all slow samples and the existing request wait. Every complete raw
output matched the sealed cold output byte for byte. The
[cache/Session controls](depth75-host-remap-integration-current.md) used the
actual learned route, verified cache isolation and corrupt-cache refusal, then
closed Session and worker before restoring/rendering the frozen result. These
prove those paths; they are not independent sensor accuracy or quality-tier
certificates.

Managed GPU buffers occupy 227,968,156 bytes in the retained plan. That quantity
does not include every process, allocator, driver or device allocation. The
host and child process observations retain their actual PIDs. Overlapping
lifetime peaks are not summed as simultaneous resident memory.

The [one-service scheduling and host counter controls](depth75-sustained-host-controls-current.md)
now pass: all four complete example tests and its build, plus an independent
three-process Windows job committed-memory peak control. These are prerequisites,
not a learned-model qualification. The subsequent
[first actual one-service run](depth75-sustained-host-remap-current.md) completed
all800 requests with exact outputs under the fixed pre-run protocol. It recorded
1,522,769,920 bytes whole-owned-job committed peak, not RSS or device memory.
All1,721 sensor readings remain: maximum90°C,16 software-thermal Active readings
and one hardware-thermal Active reading. Complete device allocation is unknown;
no production qualification is constructed from these observations.

## Regression and publication

Publication `d197552823bef0fe3bc192f01649753e059dbd8c` is draft PR #651.
The exact prior published source `53c3280d` completed CI run42573859294 with
eight applicable jobs successful and two policy skips. It does not cover the
later three retained-cohort/Rust-remap/cache-Session commits. Current-source
CI [run42582989141](depth75-ci-dc17b007-current.md) was terminal success: eight
applicable jobs passed and two policy skips. All seven new remap/memory controls
appear exactly once in each native desktop test log. This precedes the later
one-service example at27906e82 and does not replace the local full-command failure.

The later exact published source `d1975528` completed
[run42595433015](depth75-ci-d1975528-current.md): eight applicable jobs passed
and two policy skips. All eight named remap, process-memory and sustained-schedule
controls appear exactly once in each of the three native desktop test logs.
This source includes the one-service scheduler and the recorded 800-request
outcome. The CI jobs do not execute that external-model run and do not resolve
the original local failures or grant production qualification.

The original complete workspace command at `dc17b007` was session82855. It
finished with Cargo101 and consumed controller exit1. All452 expected invocations
were observed;449 normal summaries contain5,290 passes, one failure,84 ignores
and no filtering. The completed desktop target separately reported1,779 passes,
one failure,72 ignores and no filtering.
The original sampled-playback parent retained a child access violation
`0xc0000005`; no failed frame-progress assertion or native fault location is
inferred. The independent `motion-graph::editable_data`,
`motion-render::layer_rasters` and `motion-render::magnified_effect_density`
targets also exited with `0xc0000005` without normal summaries. The complete
[current-source failure record](depth75-host-remap-full-regression-current.md)
retains those missing summaries rather than counting partial output as target
success. Original source hashes remained unchanged throughout.

Earlier [full-workspace failures](depth75-full-regression-current.md),
[original retained Python-remap failure](depth75-retained-cohort-current.md),
and [sustained/alignment refusals](depth75-sustained-alignment-current.md) remain
part of the record. They are neither hidden by the changed implementation nor
reclassified by later narrow passes. All original assertions and thresholds
remain intact.

## Next acceptance boundaries

1. Diagnose the retained original full-command failures. Inspect current-source
   CI separately; the original local command remains failed.
2. Evaluate the retained actual one-service outcome without waiving its thermal
   signals or treating committed memory/managed buffers as device allocation.
   Complete conservative device memory evidence is still missing; no rerun to
   obtain cooler samples or changed qualification threshold is authorized here.
3. Establish actual independently referenced application/domain quality and
   conservative memory evidence before creating production quality bindings.
4. Establish an accepted continuous cross-quality transition under explicit
   limits, then verify automatic selection, recovery and frozen-export policy
   with those actual admitted routes. Research refusals must remain refusals.

Issue #88 retains a separate photographed-tracking failure and is not resolved
by this AI-depth work. No Android, panel, layout or interaction change is
included in these observations.
