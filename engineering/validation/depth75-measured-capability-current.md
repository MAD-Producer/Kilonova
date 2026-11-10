# Depth capability measurements (#75)

Status: the original175-target host/render/media regression and nineteen new
native controls passed at `26b9156d`. The complete28-case capability target
(including thirteen later policy controls) and actual consecutive CPU/WGSL video
probes passed at `8fdb718b`. Two original controller failures remain explicitly
retained: an unmanaged example path, then missing color declarations in the
authored fixture. Both were corrected at their respective boundaries; no asset,
color, numerical, timing or existing test assertion was relaxed. This is not
completion of #75. No Android work is included.

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
causes and reach Error file/stderr. Actual execution and its two preceding
failures are described below. The small authored-video control establishes
pipeline execution and same-model numerical equivalence only; it does not
establish photographed-video accuracy, quality-transition stability, peak memory
or sustained/thermal qualification.

## Actual native and initial model observations

Original controller session 16790 completed and was consumed with exit 1 at
`26b9156d`, with every tracked source byte unchanged. Its complete original
host/render/media command finished successfully first: 175/175 source-bound
targets, 1,628 passed, zero failed, three original ignores and zero filtering.
All 103 required named controls passed exactly once, including nineteen new
worker/latency/video controls. This is a changed-crate regression, not the
entire maintained workspace at that source.

The actual pinned Small518 CPU and Vulkan WGSL predecoded probes each completed
two uncached worker requests on the authored 12x8 RGB input. Both actual output
documents retained one JSON stdout result, zero reuse, original actual execution
identity and default Info starts before resolution and inference.

| Actual backend | All observed worker totals (ms) | Retained managed GPU buffers | Explicit 1,000 ms latency request |
| --- | --- | --- | --- |
| CPU ONNX | 16725.4254, 4644.7378 | Unknown | Precomputed required |
| WGSL Vulkan, RTX 4070 Laptop | 20241.2204, 16415.0600 | 227,968,156 bytes | Precomputed required |

No cold or slow sample was discarded. These actual child-runtime wall
observations include resolution and complete worker work; they do not promise
warm inference, realtime playback, photographic quality or thermal stability.
The insufficient-latency disposition has no resolved inference configuration.

The same original controller then encoded its three-frame authored video and
attempted the actual desktop libav probe. The probe registered `source.mp4`
outside `assets/`, so the unchanged asset validator rejected it with
`assets must be inside the assets directory`. The original Error appeared in
file and stderr, stdout stayed empty, and the probe produced no measurement.
No subsequent WGSL video or planned failure stage ran. The original failed
outputs are retained in `depth75-measured-capability-v1`.

The corrected driver now creates `assets/source.mp4` in its new owned root and
registers that same relative managed path. It preserves original-byte checks,
the asset validator, original frame selection and every numerical/timing gate.
Its independent actual verification and the fixture correction are recorded
below. The separate multi-criterion policy is documented in
[its current validation record](depth75-qualified-capability-policy-current.md).

## Corrected actual-video and policy qualification

Session45901 was consumed with exit1 at unchanged `8fdb718b`. Its complete
integration target passed28 controls (original15 plus later13), zero failures,
ignores or filtering; its actual examples and WGSL worker build also passed.
The actual decoder then refused the authored video because both primaries and
transfer were `Unspecified`. Independent `ffprobe` confirmed that the original
encoded stream and all three decoded frames lacked both declarations. The
original `-color_primaries`/`-color_trc` encoder arguments had not established
that evidence. Error file/stderr and empty stdout were verified; no inference
result was manufactured.

A separate authored-fixture correction used the pinned FFmpeg `h264_metadata`
bitstream filter to declare the intended BT.709 primaries, sRGB transfer and
BT.709 matrix. This is an explicit declaration for generated test material,
not an assumption about user media. Independent frame MD5 records confirm all
three decoded YUV planes and exact source timestamps are unchanged. The original
file SHA-256 is `01bc6b7d4b0e4deb77a3c02da39a625db97a2afbcb43ab04987f2d685733f18c`;
the separately declared fixture is
`6af90c3aa8f759f9617f73dc41efe03388c26bfd702c39d20f73396c9181c05b`.
Both original failures remain failed; no controller receipt was relabeled.

Session93439 then completed and was consumed with exit0, with the same source
and built executable identities. It reused the existing native28-case proof
and product build, without rerunning unchanged regression stages. Actual libav
registration/decode and uncached learned inference each completed two consecutive
indexed frames at64x48, preserving original bytes, indexed PTS/exclusive end,
RGB fingerprints, depth hashes, exact model/runtime identity and zero reuse.

| Actual video backend | All observed worker totals (ms) | Retained managed GPU buffers |
| --- | --- | --- |
| CPU ONNX | 21036.090, 6849.178 | Unknown |
| WGSL Vulkan | 21950.074, 20764.274 | 227,968,156 bytes |

Same-model CPU/WGSL comparisons used the unchanged bounds: normalized maximum
`2e-4` and mean `2e-5`, without fitted affine correction or clamping. For the
two complete3072-value outputs the observed maximum/mean were respectively
`9.6054973e-6 / 1.5510078e-6` and `3.1940975e-6 / 3.5511152e-7`.
These are equivalence residuals, not independent-reference model accuracy.

Corrupt-video, missing-program and original-undeclared-color controls all
returned their original errors, logged Error to file and stderr, retained
before-operation Info starts, kept stdout empty and emitted no measurement.
The undeclared original video still fails its unchanged color contract.

The [archive](data/depth75-capability-measurements-v3.zip) and
[manifest](data/depth75-capability-measurements-v3.json) retain original failures,
completed native stages, controllers, raw output logs and aggregate records.
All63 members were read back and verified:1,013,241 bytes, SHA-256
`535063525d321463f6c1f1ef2339909063c13b1b4a9e15f9be1f9f01a0a2370e`.
No weights, plans, executable binaries, media, RGB or depth tensors are included.

## Pending acceptance and remaining issue scope

- Extend the fixed sensor-reference measurements to independently admitted
  domain quality; measure conservative whole-host/device memory and longer
  sustained/thermal behavior. External policy report fixtures establish
  admission behavior only and cannot qualify real hardware or models
- Populate the existing explicit qualified worker/Session bindings with those
  independently admitted actual records before automatic selection
- Verify continuous photographed-video quality transitions before assigning
  automatic fast/standard/high mappings; backend names, resolution or successful
  timing alone do not establish a quality rank
- Complete the final maintained-workspace and exact-head remote CI gates after
  remaining source changes; preserve existing assertions, features and budgets

The separate full maintained workspace at parent `4729ab6d` finished and its
original session27030 was consumed with exit0 before session16790 started:
5,240 passes, zero failures,78 original ignores, zero filtering and all447
source-bound targets. Its separate remote CI expression-budget failure remains
unresolved. That parent workspace pass, the175-target pass at `26b9156d`, and
current28-case/video qualification at `8fdb718b` retain distinct source scopes.
None is a later complete-workspace, final CI, real thermal or #75 completion
claim.

## Later finite sustained observation

The [eight-request sensor record](depth75-sustained-sensor-current.md) adds an
actual 151.875-second desktop WGSL workload at `d68a9d65`, preserving all attempts
and 141 sampled device readings. Its 70–77 degree Celsius range and absence of
sampled thermal-slowdown flags are finite observations, not a long-run guarantee.
Whole-host memory, domain-wide quality admission and continuous transitions
remain open.

## Later independent reference and complete related regression

The [fixed sensor-reference protocol](depth75-independent-reference-protocol.md)
now retains four complete 32-request routes (CPU266/392/518 and WGSL518), two
explicit unsupported WGSL resolutions and 32 same-weight backend comparisons.
These independent sensor residuals are distinct from numerical equivalence,
and do not by themselves assign quality tiers or certify a source domain.

At `4c9c86c8`, the complete related host/render/desktop-media command passed
1,656 tests with zero failures, three original ignores and zero filtering.
All 176 targets were accounted for, including the three reference-example
controls through default Cargo discovery. Actual examples/worker builds also
passed, source hashes remained unchanged, and original session 47248 was
consumed with exit 0. The [manifest](data/depth75-reference-related-native-v1.json)
and verified archive preserve full commands and executable/source bindings.
This does not replace the older source-specific full maintained-workspace pass,
new-head cross-platform CI or the remaining measurements above.
