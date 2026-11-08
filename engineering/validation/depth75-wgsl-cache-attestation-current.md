# Issue 75: WGSL persistent-cache execution attestation

Status: in progress. This stacked branch starts at cache PR #649 head
`13675ddeabb4979e7f6b76c078adc38ec56db054`. Neither #75 nor #88 is complete.
The maintained delivery scope is desktop; Android is not being qualified.

## Transform execution identity

The native WGSL inference path uses Python NumPy/OpenCV for RGB preparation
and depth remapping. These two phases must be part of the cache execution
identity even though they do not execute the learned model. They do not need
ONNX Runtime installed.

`tools/depth_tensor.py --resolve` reports the actual numerical runtime and the
two embedded procedure sources without reading RGB/depth inputs or invoking
learned operations. `--attest <directory>` requires an explicit
`transform-execution.json` and compares that identity before reading the input
or replacing an output. It returns its observed execution identity alongside
the transform receipt. The original one-directory invocation remains unchanged.

The runtime digest includes the selected packages' Python/native code, relevant
sibling numerical libraries, CPU dispatch features, Python/OS/build information
and bounded numerical environment. Package sites may differ; dependency
enumeration order must not change the identity. Enumeration, source bytes,
numerical context and expected-identity metadata are bounded. Raw environment
values are hashed and are not emitted in receipts or logs.

CPU ONNX now uses the same bounded numerical-runtime helper with its own domain
and ORT build identity. Its hash domain advances to v2; previously computed
identities conservatively miss rather than being treated as interchangeable.
No old-project migration or compatibility fallback is introduced.

## Evidence and limits

- The existing `test_ci_depth_tensor.py` and `test_ci_depth_export.py` tests are
  preserved. New tests are in `test_ci_depth_tensor_identity.py`
- The Python depth controls pass: 24 tests, including real child-process
  protocol tests with an import guard that rejects ONNX Runtime imports
- The synthetic-package controls prove identity/admission behavior, not actual
  learned inference, GPU execution or model quality
- An actual installed NumPy/OpenCV transform comparison passed for both RGB
  preparation (3,219,888 bytes) and remapping (140 bytes), matching the previous
  procedure byte for byte. An altered expected identity was rejected without
  replacing an existing output. The v2 receipt repeats this on the current
  bounded-context implementation, with source hashes unchanged
- Scoped host/cache regression passes: five requested Rust targets, 277 passed,
  zero failed, 40 original GPU-dependent ignores and zero filtered. GPU features
  were not requested for this scoped invocation; these ignores are not GPU
  acceptance. No complete host/render/media regression pass is claimed
- The shared helper is also verified with the actual separately installed Small
  266 CPU ONNX model. The original file-cache probe assertions are unchanged;
  only the harness source paths point to this branch. One inference populated
  the cache, a new worker restored all 96 scalar bit patterns, and a corrupt
  entry was refused without another inference. Default Info publication before
  the controlled handler returned, Error file/stderr and single-JSON stdout
  isolation passed. This synthetic input does not qualify model/video quality,
  sustained performance or native WGSL caching. Receipt:
  `artifacts/pr-queue/depth75-wgsl-shared-runtime-cpu-model-v1`

The actual numerical receipt is local under
`artifacts/pr-queue/depth75-wgsl-transform-validation-v2`; the scoped regression
uses `artifacts/pr-queue/depth75-wgsl-host-identity-contract-v1`. These are independent
evidence stages and do not replace the original failed full regression on
PR #649. That PR remains a draft despite green remote checks.

## Host execution boundary

The product host now resolves both procedures before cache lookup. It combines
the observed native execution, NumPy/OpenCV execution and actual configured
worker-file digest. A miss requests attested preparation, native inference and
remapping; each actual receipt must agree with the independently resolved
identity. A hit still resolves actual hardware, numerical runtime and all
282 pinned model resources before accepting raw values. It does not reuse a
cached GPU object or bypass resource ownership/alignment admission.

The native worker now has separate `--resolve` and `--attest` entry points.
Resolution requests have a strict input-free schema; inference requests retain
their existing input tensor contract. Before publishing a native identity, the
source verifies every admitted pinned weight resource and includes its own
executable digest, the actual adapter/driver metadata, required device limits,
explicit Float32 procedure and bounded native numerical environment. The
attested inference path compares that identity before learned operations.
Missing driver identity, software adapters and changed execution are refused;
the ordinary inference invocation remains available independently.

Current product worker sources compiled in a minimal isolated harness using
only verified lockfile dependencies. Its 16 admission tests pass, including
worker/driver/backend/limit/program identity and bounds controls. A real default
Info child invocation retained the original invalid-request failure in both
the file log and Error stderr, with an empty stdout and zero encoded model
operations. These checks do not exercise successful hardware resolution or
actual attested model inference. The harness is not a product-package build.
Receipt: `artifacts/pr-queue/depth75-wgsl-native-identity-contract-v2`. The v2
source retains the ordinary invocation's original version-before-shape error
precedence; the earlier v1 receipt remains a separate historical stage.

Eight new host unit controls cover the three independent identity components,
wrong backend/precision/procedure, partial or stale resolution, missing or
changed actual receipts, cancellation clearing stale state before scratch IO,
and a changed actual worker file. Existing assertions remain unchanged.
Cancellation, primary/cleanup causes, directly owned children and in-flight
publication retain their existing contracts. Starts are flushed before blocking
children or worker-file identity reads; healthy Present behavior is unchanged.

## Actual hardware and fresh-worker cache proof

The pinned 518 Small export was recovered from an existing local installation,
not downloaded or added to the product. All 282 resource entries (238 unique
files) matched their blueprint dimensions, Float32 byte counts, finite values
and SHA-256 digests. The initial resource-restoration v1 failed because the
optional ONNX inspection package was absent; its output remains preserved.
Restoration v2 used an isolated external inspection-tool directory, leaving the
actual NumPy/OpenCV/ORT inference environment unchanged. Resource receipt:
`artifacts/pr-queue/depth75-wgsl-cache-resources-v2`.

The current production host and native-worker source then passed a real Vulkan
execution on an NVIDIA GeForce RTX 4070 Laptop GPU. The external probe delegates
resolution and inference to the actual WGSL backend; it does not supply model
results. Its cold miss passed the production receipt requirements (429 encoded
operations, one final readback, 282 verified resources and the pinned retained
buffer contract). A newly constructed worker restored all 96 scalar bit patterns
with a new resource ownership identity, zero inference cost and only one backend
inference call across both workers. Corrupting the cache caused the original
content-digest error without another inference.

The controlled pre-handler pause proved that `cache_resolve` in-flight state and
its default Info file record were visible before the backend returned, without
a fixture-side logger flush. The actual corrupt-entry Error appeared in both
the file and stderr; stdout contained exactly one fixture JSON record.

Single-sample descriptive timings were 25,806.6975 ms cold full path,
12,692.9692 ms reported native inference and 6,887.5837 ms new-worker hit full
path. Cold excludes the controlled publication pause; hit includes actual
resource/hardware/runtime resolution. These measurements concern one 12-by-8
synthetic input and do not qualify visual quality or sustained performance.
The source hashes remained unchanged. Receipt and independently authored probe:
`artifacts/pr-queue/depth75-wgsl-cache-actual-v1` (actual session 77445, exit 0).
Both binaries used isolated manifests with exact root-lockfile dependency
checksums; this proves the current-source execution, not a full product-package
build.

## Actual cached admission and recovery controls

A separate current-source probe reopened an owned copy of that cache and of the
pinned model resources. It restored the original corrupted cache byte only in
the new copy; the original corruption evidence remains unchanged. Actual GPU,
resource and numerical-runtime resolution preceded every lookup. It passed:

- Three raw-cache hits with zero WGSL backend inference calls, including the
  reference, an aligned candidate and recovery after a refused request
- Fresh worker-side alignment with 48 held-out validation pairs, original
  provider/prepared-pixel identity and zero inference cost. Invalidating the
  live controller rejected that retained proof despite identical public keys
- A changed actual weight byte refused during `cache_resolve` before lookup.
  The original resource name/digest cause reached the default file and Error
  stderr. It did not become a cache hit or trigger inference. Restoring the
  owned weight allowed a later request to hit again
- Cancellation after real hardware/resource/runtime resolution, while a
  controlled wrapper still held the backend return, published neither the
  existing cached result nor a failure. Queue/in-flight state drained, no
  inference ran and the owned temporary directories were empty. This control
  proves cancellation before lookup; it is not live-child cancellation
- An independent current CPU ONNX inference for the exact same 96-pixel input
  agreed with the retained actual GPU output at maximum normalized error
  `0.0000020520434330588136` and mean `0.0000007814865407565651`. Original
  maximum `0.0002` and mean `0.00002` limits were unchanged. The normalization
  is `max(max(abs(cpu_reference)), 1e-6)`; no fit or clamp is applied to this
  numerical comparison

Receipt: `artifacts/pr-queue/depth75-wgsl-cache-boundaries-v1`, actual session
67097, exit 0. The probe used the existing qualified native binary and exact
lockfile dependencies; source hashes remained unchanged. Its single stdout JSON
record remained separate from actual Error stderr. It does not establish
photographic/video quality, thermal stability or another GPU/backend.

Acceptance still requires remaining actual execution-mutation controls and
broader alignment/video qualification, full regression, product-package
builds and exact-head cross-platform CI. Broader #75 video
quality, backend comparison, full-path latency, memory and sustained-load
qualification remain separate obligations. No panels or UI behavior is changed;
no product dependency or third-party asset is added. The local inspection-only
tools and their license scope are recorded in `ATTRIBUTION.md`.

## Maintained-main product build and complete changed-crate gate

After aligning with main `ac65d279c57626d7aa7e28061a1a7fdf7889c68a`, source
`d853223c3893457719f54964ab62b069c6cb63f0` passed the desktop check and actual
`motion-desktop` package build of `motion-depth-wgsl`, with FFmpeg and both host
and desktop-media AI-depth features. The complete original host/render/media
GPU/FFmpeg command then passed: 1,609 tests, zero failures, three original
ignores, zero filters, and all 174 expected test targets accounted for by Cargo
metadata and source-bound dep-info. All 84 named depth controls passed. All
tracked source hashes were unchanged. Actual session 24454 was consumed with
exit zero. Receipt: `artifacts/pr-queue/depth75-wgsl-cache-product-v2`.

This is not the complete maintained-workspace or desktop integration gate.
Historical native crashes on other source snapshots remain unresolved; this
successful run does not establish their root cause or remove their records.

The subsequent standalone consumer fixture did not reach inference: its build
failed when rustc exited with `0xc0000005` while compiling `motion-effects`.
Actual session 57772 was consumed with exit one. The original output remains in
`artifacts/pr-queue/depth75-wgsl-cache-product-actual-v1`; dependent controls were
not launched. Neither a model failure nor a compiler-crash fix is inferred.

## Reproducible workspace acceptance drivers

The following manual examples now use the real workspace dependency graph and
root lockfile. They retain the original actual-cache fixture assertions,
numerical bounds, deadlines and controlled publication barriers. They delegate
all inference and cache admission to the production host; no model result is
provided by a fixture. They are not model-free CI tests. Their addition alone
does not qualify a successful real-model run.

```text
cargo build --locked -p motion-desktop -p motion-host \
  --bin motion-depth-wgsl --example wgsl_depth_cache \
  --example wgsl_depth_cache_boundaries --example wgsl_depth_cache_identity \
  --features motion-desktop/ffmpeg,motion-host/ai-depth,motion-desktop-media/ai-depth
```

Use separately installed, licensed pinned Small 518 model resources and the
existing Python numerical runtime. Supply absolute paths with `MOTION_LOG`
unset; output directories belong exclusively to these controls:

- `wgsl_depth_cache <new-output-root> <python> <model.onnx> <program.json>
  <native-worker>` performs one cold inference, reopens the cache in a new
  worker, compares all scalar bits and fresh resource identity, then deliberately
  corrupts its own entry and requires refusal without another inference. Its
  controlled blocking and file/stderr/stdout assertions remain unchanged
- `wgsl_depth_cache_boundaries <owned-output-root> <python> <model.onnx>
  <copied-program.json> <native-worker> <cold-proof-root> <copied-weight-file>`
  requires an owned repaired copy of the cold control's cache. It checks fresh
  alignment and generation, resource refusal/recovery, cancellation before
  lookup and independent CPU numerical agreement. Both mutable resource paths
  must reside inside the owned output root
- `wgsl_depth_cache_identity <owned-output-root> <python> <model.onnx>
  <program.json> <copied-worker>` requires an independently copied worker and
  its runtime libraries inside the owned output root. It changes that copy
  after actual resolution, requires refusal before native inference, restores
  its bytes and independently resolves again. It refuses a worker outside the
  owned root

The cold control leaves its cache deliberately corrupt as failure evidence.
Boundary setup repairs the single changed byte only in another owned copy;
never reuse that evidence directory as a normal application cache. The
identity control must never receive an installed application executable.
No asset, dependency, UI or normal backend behavior changes accompany these
explicit acceptance tools.


## Current workspace product and complete maintained-workspace result

These results supersede the earlier pending status for the workspace examples
and local complete gate only. The qualified source is
`4729ab6d1c7c0c535c9715a53bc79497a78970b4`, based on maintained main
`ac65d279c57626d7aa7e28061a1a7fdf7889c68a`. Later capability-selection source
is outside this evidence.

The actual root-workspace product build and all three real-model cache drivers
passed, with source hashes unchanged (original session 65121, consumed exit 0).
The pinned Small 518 model ran on the RTX 4070 Laptop Vulkan adapter. A cold
inference, a fresh-worker bit-exact restore of all 96 values, corrupt-cache
refusal without inference, three boundary cache hits, fresh alignment with
48 held-out pairs, resource mutation/refusal/recovery, and cancellation before
lookup passed. The independent CPU comparison retained the original maximum
`0.0002` and mean `0.00002` normalized limits; observed errors were
`0.0000020520434330588136` and `0.0000007814865407565651`. Changing an exclusively
owned worker copy after actual resolution refused execution before native
inference; restoring its bytes allowed independent resolution again. Default
Info start was visible before handler return, Error reached file and stderr,
and stdout remained one JSON record. These are cache and execution-boundary
proofs on one synthetic 12-by-8 input, not photographic/video quality evidence.

The complete maintained-workspace command then passed on Windows (original
session 27030, consumed exit 0): 5,240 passed, zero failed, 78 original ignored,
zero filtered. All 447 expected invocations were independently derived from
Cargo metadata and matched the actual executable dep-info or doctest owner;
all 84 named depth controls passed exactly once. There were no missing or
unexpected targets, no abnormal target exits, and all tracked sources remained
unchanged. Android was excluded as requested. The command was:

```text
cargo test --workspace --exclude motion-android --locked --offline \
  --features motion-desktop/ffmpeg,motion-host/ai-depth,motion-desktop-media/ai-depth,motion-host/gpu-tests,motion-render/gpu-tests,motion-ui/gpu-tests,motion-desktop/gpu-tests,motion-desktop-media/gpu-tests,motion-graph/gpu-tests \
  --no-fail-fast -- --test-threads=1
```

Public text-only evidence, including source binding, raw logs, controllers,
terminal receipts and archive member digests:

- [Actual workspace product proofs](data/depth75-wgsl-workspace-product-v1.json):
  41 verified archive members, SHA-256
  `0f25c398f44ba1e79c58fc56a492f390cb621b4cc8c09a7f8e5ec4be9e7dcf29`
- [Complete workspace result](data/depth75-wgsl-maintained-workspace-v1.json):
  11 verified archive members, SHA-256
  `1c379960158bbde6442c7ef9560e14d9bc37e06c6782b83a0fd07c52384c2b3f`

### Remote CI and remaining limits

Exact-source workflow `42456771015` completed with failure. All three desktop
native jobs and all three desktop builds passed; dependency audit also passed,
and two jobs were skipped by existing policy. The Rust/GPU job failed the
original `whole_vectors_separated_axes_and_opacity_use_evaluated_property_units`
test: `bindings_and_module frame_deadline`, elapsed 105,751 microseconds versus
99,962 microseconds remaining at entry. Its actual tested merge tree matched
this source tree. The overrun's cause is unproven. No assertion, budget or ignore
was changed and no CI retry was requested. The local complete pass does not
turn that failed workflow into a remote pass or prove a timing fix.

Historical standalone compiler and native access violations remain preserved
with unresolved causes. The later workspace product pass is not a crash-fix
claim. The archives contain no model weights, executable or library binaries,
RGB inputs or depth tensors.

Issue 75 still requires measured capability policy, broader actual source/video
quality, full-path latency, memory and sustained-load/thermal qualification.
Issue 88 retains its failed photographed tracking-quality gate. Neither issue
is complete; this increment adds no panels, UI behavior, product dependencies
or third-party assets.
