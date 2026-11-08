# Depth capability measurements (#75)

Status: implementation prepared; native tests and real-model qualification are
pending. This is a prerequisite for measured selection, not completion of #75.
No Android work is included. The cache qualification on the parent commit does
not qualify these new sources.

## Construction and timing boundary

`Jobs::start_measured` explicitly creates a probe worker. The ordinary and cached
constructors retain their existing roles. A probe uses the existing bounded
queue, cancellation, generation/current-key checks and panic handling. Before
calling the runtime resolver it publishes `capability_resolve` in-flight state
and flushes an Info start through the existing file sink. It publishes another
Info start before actual inference. No business-state lock spans the resolver,
inference or logging.

Every probe requires an actual execution identity. Missing/invalid identities,
changed returned providers, wrong result keys and original backend failures
remain errors. Cancellation after resolution prevents inference even if a
backend neglected its own cancellation check. A panic retains its cause and
stops this worker; there is no retry or backend fallback.

Only a validated uncached result can carry `capability::Measurement`. Its
constructor is private to the worker. The measurement stays paired with the
worker-owned prepared-input fingerprint and exact resource key. Ordinary
successes, persistent-cache hits and session spill restores have no measurement.
Probes neither restore nor write reusable results. A private worker identity and
request number distinguish independent observations; cloning a measurement
does not create another sample.

`total()` is actual worker wall time: source preparation and hashing, actual
execution resolution, staging, inference, output parsing/ownership validation
and runtime cleanup. `resolve()` is a subset of that total. The backend's claimed
total is overwritten by the worker. Queue wait, caller-side decode/preparation,
alignment and downstream installation are outside this measurement. Existing
backend phase costs remain available separately; their input/output tensor byte
counts are not peak memory measurements. Input geometry, model identity and
actual execution identity must accompany all published comparisons.

Probes additionally preserve the backend's observed retained managed GPU buffer
capacity. WGSL publishes it only after validating the actual native receipt,
the complete output and cleanup. That receipt checks all retained WGPU buffer
sizes against the independently admitted fixed pool. New resolution/inference
attempts clear an older observation before cancellation or IO. The getter is
bound to the last successful input key; `Routed` delegates only to the selected
backend. ONNX currently reports `None`, explicitly unknown rather than zero.
Zero and values above the current native 512 MiB contract are refused by probe
admission. Profiles also reject a changed retained-capacity observation.

This value covers the learned native inference's managed buffers. It excludes
host allocations, driver overhead, preprocessing/remapping allocations and
other processes. It is not an OS/device memory peak or a memory fitness verdict.
Those remaining measurements must precede memory-based automatic selection.

`capability::Profile` retains at most 64 observations and never keeps RGB or
depth planes. It accepts only worker-measured results for the same exact model,
geometry, actual provider, runtime, plan and numerical precision. Duplicate
worker/request observations, ordinary results, identity changes and capacity
overflow are rejected atomically. Every retained observation exposes its source
and prepared-input fingerprint. A different worker's request number is a
different observation; cloning one result is not.

The read-only summary reports all retained observations' minimum, upper median,
maximum and maximum resolution duration. It removes no warm-up or outlier. These
are observed order statistics, not future latency guarantees or proof that a
caller-selected sample set represents a video. The real-model example records
its results through this same profile admission path.

## Reproducible probes

The manual `depth_capability` example uses the actual workspace dependency graph
and root lockfile. It requires separately installed, previously attributed
licensed model resources and the existing numerical runtime; it embeds none.
It accepts 1–16 serial samples and a bounded predecoded opaque RGB8 sRGB file.
It emits one JSON result on stdout and writes `measurements.json` into a new
application-owned output directory. Normal logs use the existing file sink;
Error records also reach stderr. It requires `MOTION_LOG` to be unset.

```text
cargo build --locked -p motion-host --features ai-depth --example depth_capability
depth_capability <new-output-root> <onnx|wgsl> <python> <model.onnx> \
  <program.json|-> <native-worker|-> <rgb-file> <width>x<height> <samples:1..16> \
  <maximum-worker-latency-ms:1..60000>
```

WGSL additionally requires an actual compatible `motion-depth-wgsl` build and
the pinned plan/weights. The ONNX mode explicitly requests CPU; no requested
provider is substituted for the returned provider. A new child runtime is
resolved and used per request under the existing backend contracts, so these
are not warm-session latency promises. The example's external 60-second wait
only bounds this explicit acceptance driver; it is not a product per-command
timeout or worker restart.

## Latency admission stage

`capability::latency::select` accepts at most eight measured profiles and an
explicit exact-model, geometry, optional backend, minimum-sample and maximum
worker-latency request. It preserves that request independently of its resolved
model and actual execution identity. A requested unavailable backend does not
select another backend. Comparable profiles must retain the same ordered
source/revision/PTS and prepared RGB fingerprints; equivalent rational PTS is
accepted. Repeated executions, empty profiles, unpaired inputs and malformed
requests are errors. All candidates retain their observations and individual
admission reasons.

Among eligible profiles, selection minimizes the observed maximum, then upper
median, preserving caller order on exact ties. No observation is discarded.
Exactly meeting the maximum is eligible; exceeding it by any amount is refused.
Insufficient evidence, missing configurations or no configuration within the
budget produce `PrecomputedRequired` with no resolved inference configuration.
This reports a required alternative, not a loaded precomputed result or AI
success. No inference, retries, IO or product configuration change occurs.

The actual probe driver runs this stage on its measured profile and emits its
explicit budget, request, resolved identity or precomputed disposition. Its
minimum is two samples; one-sample runs remain observable but cannot pass this
admission. The independent controls cover paired-input refusal, requested-backend
isolation, actual provider/numerical identity, bounded requests, exact budget
boundaries, insufficient samples and the no-evidence alternative.

This is one necessary stage of capability selection. Quality/domain, complete
memory and sustained/thermal qualification remain prerequisites before any
fast/standard/high mapping or automatic product selection is authorized.

## Original-video measurement boundary

`VideoInput`, `VideoKey`, `Jobs::request_video_sample` and
`Jobs::take_video_sample` expose the existing registered-original-video worker
path, without a Session, render handle or persistent spill. The public result
also exposes its decoder-verified exclusive source interval end. The independent
live request comparison includes object, asset/revision, target coordinate,
geometry and exact model; a changed target within the same decoded PTS interval
still discards an obsolete completion. An arbitrary target coordinate is never
reported as a decoded PTS.

With `start_measured`, managed index validation, actual decoder creation/use/drop,
SDR transfer conversion, crop/orientation and prepared-RGB hashing are included
in source preparation and worker total. The original bounded staging, source
containment, byte-count, indexed PTS/end/dimensions and color checks are reused.
Malformed decoding fails before inference or measurement construction. The
controlled-video tests separately exercise a held decoder-open operation,
worker-thread ownership, preparation timing, same-PTS uncached repetition,
decoding refusal and live-request invalidation. Their decoder and inference are
controlled; these tests alone are not actual libav/model qualification.

The manual media example uses the actual desktop libav implementation:

```text
cargo build --locked -p motion-desktop-media --features ai-depth,ffmpeg \
  --example depth_video_capability
depth_video_capability <new-output-root> <onnx|wgsl> <python> <model.onnx> \
  <program.json|-> <native-worker|-> <source-video> <start-index> <count:1..64>
```

It verifies an owned byte-identical copy of the original and registers an actual
probed source/index before measuring consecutive indexed frames. It performs no
transcode or skipped-frame selection. Unsupported original codecs remain errors.
Source-copy/probe registration is timed separately. Each sequential request
decodes on the worker and establishes actual PTS, exclusive end, RGB fingerprint,
model and execution identity; reuse counts must remain zero. Full scalar output
is retained as hashed little-endian float files with descriptors, under a total
128 MiB archive bound. The profile retains at most 64 observations and RGB/depth
planes are released between samples. The explicit manual-driver wait remains
60 seconds, without changing any product worker deadline or adding restarts.

The single stdout result and `measurements.json` retain observed whole-worker
and backend phase timings. Default file logging publishes starts before source
registration/probing and before worker decode/inference; failures retain original
causes and reach Error file/stderr. This is prepared source, not yet a successful
actual-codec run. Even a successful small authored-video control would establish
pipeline execution only; it would not establish photographed-video accuracy,
quality-transition stability, peak memory or sustained/thermal qualification.

## Pending acceptance and remaining issue scope

- Run all nineteen new native controls, including a resolver held at a controlled
  barrier, cancellation, identity/provider refusal, backend error/panic,
  cache exclusion, duplicate observation identity, bounded profile statistics
  and atomic identity/capacity refusal, unknown/bounded memory observations,
  exact route delegation, stale WGSL memory reset before cancelled work and the
  bounded latency admission stage and original-video measurement boundary
- Run the relevant original host/depth regressions without changed assertions,
  GPU policy, original ignores or deadlines
- Run actual CPU and WGSL probes against pinned resources, bind their output to
  source/lock/runtime identity, and verify default-level file starts, Error
  stderr and stdout isolation on real failure as well as success
- Run the actual libav video example on consecutive source samples for CPU and
  WGSL, verify preserved original bytes/PTS/RGB/depth ownership and phase costs,
  then perform the separately required real-video/quality-transition benchmark
- Qualify the implemented bounded profile aggregation and latency admission,
  then combine them with quality/memory/sustained evidence in a complete measured
  policy with requested/resolved backend/model and precomputed-depth disposition
- Qualify accuracy/domain suitability, conservative memory admission, sustained
  execution/thermal behavior and continuous-video quality transitions before
  assigning fast/standard/high. Neither backend name, model input resolution nor
  a single successful timing establishes a quality rank or device capability

The full workspace regression currently running on parent `4729ab6d` must finish
and have its actual session consumed before another own heavy workload starts.
That source tree remains unchanged while this independent worktree is edited.
