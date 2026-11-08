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
  <program.json|-> <native-worker|-> <rgb-file> <width>x<height> <samples:1..16>
```

WGSL additionally requires an actual compatible `motion-depth-wgsl` build and
the pinned plan/weights. The ONNX mode explicitly requests CPU; no requested
provider is substituted for the returned provider. A new child runtime is
resolved and used per request under the existing backend contracts, so these
are not warm-session latency promises. The example's external 60-second wait
only bounds this explicit acceptance driver; it is not a product per-command
timeout or worker restart.

## Pending acceptance and remaining issue scope

- Run all seven new native controls, including a resolver held at a controlled
  barrier, cancellation, identity/provider refusal, backend error/panic,
  cache exclusion and duplicate observation identity
- Run the relevant original host/depth regressions without changed assertions,
  GPU policy, original ignores or deadlines
- Run actual CPU and WGSL probes against pinned resources, bind their output to
  source/lock/runtime identity, and verify default-level file starts, Error
  stderr and stdout isolation on real failure as well as success
- Implement bounded profile aggregation and an explicit measured selection
  policy with requested/resolved backend/model and precomputed-depth disposition
- Qualify accuracy/domain suitability, conservative memory admission, sustained
  execution/thermal behavior and continuous-video quality transitions before
  assigning fast/standard/high. Neither backend name, model input resolution nor
  a single successful timing establishes a quality rank or device capability

The full workspace regression currently running on parent `4729ab6d` must finish
and have its actual session consumed before another own heavy workload starts.
That source tree remains unchanged while this independent worktree is edited.
