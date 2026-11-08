# Explicit multi-criterion depth admission (#75)

Status: prepared implementation and twelve appended integration controls;
native execution and real qualification remain pending. Parent source
`26b9156d` is undergoing its independent original regression/model/video gate.
Its frozen worktree has not been edited to add this stage. No Android, panels,
default model distribution or dependency changes are included.

## Scope and evidence boundary

`depth::capability::policy` admits at most eight explicitly supplied measured
profiles. Each request retains its `fast`, `standard` or `high` label, domain,
hardware/environment record digest, geometry, optional exact model and backend,
sample count, worker latency, whole-host/device memory limits, independent
reference residual and additional transition-jump limits, sustained duration
and maximum temperature. There are no invented default limits or implicit
model/resolution/backend-to-quality mappings. The caller supplies the qualified
application limits for the requested tier.

Latency observations still come exclusively from the actual measured Jobs
worker and retain every observation, including cold/slow executions. Different
model profiles may be compared only on the same ordered source/revision/PTS
and prepared RGB cohort. A versioned digest canonicalizes rational PTS, so an
alternate equivalent clock representation does not invalidate pairing.
Duplicate actual model/geometry/execution configurations are rejected rather
than counted as independent choices.

Independent-reference quality, complete memory and sustained/thermal reports
are **trusted external caller inputs**. This module neither measures these
quantities nor authenticates a report from its digest. The host must inspect
and verify the referenced records before supplying them. A nonzero digest is
an audit identity, not evidence that a measurement occurred. The controlled
reports in the tests establish admission behavior only; they do not qualify
any real model, domain, hardware, memory peak or temperature.

Every report binds the exact model/hash/processing, geometry, actual execution
provider/runtime/plan/precision, ordered cohort, environment and domain. Quality
reports must cover the entire measured cohort, without a smaller cherry-picked
sample count or an invented larger count. Quality residuals refer to an
independent reference under the identified protocol, not same-model ONNX/WGSL
numerical equivalence. The separate additional-transition-jump measurement is
not inferred from a single-frame residual.

Memory reports distinguish host and device peaks, weight bytes and peak
activations. Their complete-device observation cannot be smaller than the
worker's actual known retained GPU buffers. An unknown managed capacity is
not converted into a physical zero; any whole-memory report remains a separate
caller-verified input. Sustained records retain attempted sample count,
failures, cancellations, duration, worst latency and maximum temperature.
Canceled or failed attempts refuse sustained admission rather than disappearing
from the denominator. A request duration cannot be shorter than its measured
worst operation, and impossible counts/nonfinite values are invalid reports.

## Decision behavior

Each candidate preserves its measured summary, the supplied report and all
individual refusal reasons. Missing quality, complete memory or sustained data
remain unknown. Exceeding an exact request limit refuses that dimension; exact
equality is eligible. Requested model/backend/domain/environment remain
independent from resolved actual model and execution. An unavailable explicit
backend does not select another backend.

Among fully admitted candidates, selection minimizes observed worst worker
latency, then upper median, with stable caller order on exact ties. Empty input
or no fully qualified candidate returns `PrecomputedRequired` and no resolved
inference configuration. This disposition requires a separately supplied
precomputed resource; it does not create one or claim AI success. The pure stage
performs no IO, logging, inference, installation, retries, fallback or product
configuration change.

## Prepared verification and remaining work

The twelve appended `capability_policy_*` controls use actual measured Jobs
profiles with controlled backends. They cover exact boundaries, all three
requested labels, unknown sections, requested/actual identity, every reported
limit, source/model/execution/report mismatch, complete quality sample counts,
failed/canceled sustained attempts, invalid or nonfinite reports, cross-model
comparison, rationally canonical cohorts, bounded requests/candidate counts,
the actual managed-buffer lower bound, and worst-case selection without
discarding samples. The original fifteen integration controls remain unchanged.

Formatting, syntax parsing and diff checks are preparatory evidence only. Do not
claim these twelve controls passed until their original native command reaches
a verified terminal result. Continue using one own heavy workload at a time.

Real independent domain quality, conservative whole-memory observations,
sustained/thermal records and continuous-video transition acceptance remain
required. The host has not been switched to automatic selection, no real report
has been invented and no unresolved #75 or #88 acceptance item is closed by
this prepared admission stage. Integration into the existing transition,
installation and frozen-export path must consume actually qualified choices.
