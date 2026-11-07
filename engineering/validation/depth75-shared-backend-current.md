# Optional WGSL depth backend: shared jobs and real outputs

Issue #75 remains open. An explicit optional `depth::wgsl::Wgsl` backend now
implements the existing host `Backend` contract and returns an ordinary owned
`DepthResource` through `Jobs`. No desktop UI, automatic backend/quality policy,
SDK compute capability, weight installation or default release packaging is
enabled. The complete source regression and exact-head cross-platform CI remain
separate pending gates.

## Execution and ownership

The desktop package builds a separate `motion-depth-wgsl` executable. Its
operator shaders, graph admission, index lowering and graph execution are
byte-identical to the composed research qualification. The original 31 host
admission assertions are retained; only their module paths change. The existing
application logger's production implementation moves unchanged into
`logging/sink.rs`, shared by the editor and the worker. Original `logging::tests`
names, assertions, test-module paths, file/Error-stderr/stdout rules and panic
hook behavior are retained.

The host owns three sequential, directly cancellable subprocess phases:

1. RGB8/sRGB preparation using the unchanged ONNX bicubic letterbox,
   normalization and NCHW procedure
2. Native WGSL inference using the exact pinned static plan and external,
   independently hash-checked Float32 resources
3. Scalar inverse crop and align-corners remapping using the unchanged ONNX
   output procedure

Neither transform child launches an inference grandchild. Existing bounded
pipe capture terminates and reaps the directly owned child before returning
cancellation. The inference process owns its separate device; no render Session
or editor GPU handle crosses that boundary. Before temporary cleanup, failures
retain the bounded native primary-cause receipt as well as the original child
exit/diagnostic; a long backtrace tail cannot displace the primary model cause.

`Jobs` publishes its request/source in-flight state before entering the backend.
Each child phase has an Info start before spawn/wait, followed by Debug timing.
The worker's own receipt exposes more detailed resource/submission phases. The
host's normal job polling adds no per-poll logging. No Present path is changed.

## Initial source-bound execution

The base is maintained main `daf806b5f29c2ec0a70e41068445230fb7b13585`.
The archive identifies each changed source and the exact native worker and
acceptance-driver binaries. Earlier v1 tests and final v2 execution are retained
separately. Final changes only add bounded failure retention, its regression,
manual cancel/stale controls and remove an unused import; the compute code and
original worker/logger controls remain unchanged.
The final staged whitespace check also removes one redundant terminal blank
line from the extracted sink; its archived tested source retains that blank.

| Actual gate | Result |
| --- | --- |
| Initial host protocol/ownership/failure controls | 6 passed; other 217 host tests filtered, none ignored |
| Original worker admission controls | All 31 passed; none filtered/ignored |
| Existing logger regression subset | All 7 passed; other 1,825 editor tests filtered, none ignored |
| Python transform contracts and actual failed children | All 3 passed, including normal and optimized Python |
| New desktop native worker and host driver | Both actually compiled; no new dependency or lockfile change |
| Complete `test_ci_*.py` discovery | 113 passed and one existing environment skip; 114 discovered |
| Provenance, validation inventory, panel-token guard and diff check | All passed |
| Authored 93-by-157 portrait through actual Jobs | Maximum normalized error 0.000002931845; mean 0.000000162665 |
| Existing photographed source, cropped/rotated/mirrored to 520-by-790 | Maximum normalized error 0.000033791168; mean 0.000000463264 |
| Original numerical bounds | Maximum 0.0002, mean 0.00002; no affine fit, threshold change or replacement of CPU reference |
| Returned source/geometry/model and prepared RGB fingerprint | Matched independently generated identities; one completion, zero failed/stale, queue drained and in-flight cleared |
| Actual corrupted program and missing actual weight resource | Both refused; primary causes retained in default file and Error stderr, empty stdout |
| Cancellation after actual native upload entry | Actual child verified alive before cancellation and terminated afterward; canceled=1, failed=0; no depth installed, scratch empty |
| Actual completed result taken with a changed live source revision | Stale=1, completed=1; neither the changed nor original key obtains the consumed result; no depth installed |

The photographed case reuses the existing attributed NASA Destiny laboratory
fixture (`e8ed893f990ba8850000064e153d57b84e6e1d5c0a0aa2ccc8dfd1254b2e379d`).
Preparation applies crop `[70,50,790,520]`, a clockwise quarter-turn and horizontal
mirror. Asset/revision, rational PTS and original geometry remain in the returned
resource. This is a numerical agreement check, not recovered physical depth,
ground-truth scene accuracy, continuous motion or an additional image asset.

Both final GPU predictions reproduce the earlier GPU output hashes exactly.
The two independently retained CPU outputs are reused after verifying their
SHA-256, rather than rerunning a reference until it passes. The adapter is NVIDIA
GeForce RTX 4070 Laptop GPU, Vulkan, driver 591.86. Metal and DX12 remain explicit
request options without actual-device qualification by this record.

## Costs and remaining gates

The first observed backend calls cost 9.764 and 6.350 seconds, including cold
subprocess initialization, preparation, device/pipeline creation, uploads and
remapping. They are not interactive/fast-tier measurements. Each native child
currently creates a device and loads resources again. `Costs.inference` includes
the whole native child; preparation and remapping include their own child wall
times. `load`, `input_copy` and `output_copy` are not independently measured and
remain zero. Tensor byte counters identify the explicit normalized input/final
native output, not physical device transfer accounting or zero-copy evidence.

Outstanding #75 work includes measured quality/resolution tiers, requested and
resolved model/backend capability negotiation, hysteresis and generation rules
for dual-backend quality transitions, versioned/frozen export policy acceptance,
continuous video drift/jump tests, sustained/thermal measurements, other desktop
GPUs, complete source regression and final CI. The standalone pinned blueprint
is not accepted by msfx or declared a generic executable package capability.

The separate #88 full regression remains failed twice, and its photographed
tracking quality gate remains failed. This optional backend does not repair,
waive or replace those gates. Android remains retired and excluded.

The [evidence archive](data/depth75-shared-jobs-v2.zip) and
[manifest](data/depth75-shared-jobs-v2.json) retain both source-bound native
qualifications, exact sources, unchanged base preprocessing/lockfile, actual
default-level logs, independent numerical comparisons, and original failure,
cancellation and stale-result receipts. All members were read back and verified:
202,760 bytes, SHA-256
`31db7cdf6dfa67b84c4253317506cde2267b666f455d66e95c9236bd0a86004f`.
Raw images/RGB/scalar tensors, model/weights, executable/PDB and optional runtimes
are excluded. Frozen observers retain explicit local routing; reproduction
elsewhere must record routing changes and install the pinned external resources.

The separate [static-gate archive](data/depth75-static-final-v1.zip) and
[manifest](data/depth75-static-final-v1.json) retain actual full tool-discovery
output and the other static gates. SHA-256 is
`efe770726cf0f0995d7b16ff3a46f45062fbbf71819990c9923d2b07c9f3aaa0`
(9,283 bytes). An earlier system-Python attempt failed two imports because it
lacked the existing CI PyYAML dependency and used the system temporary directory.
That failure is recorded, not counted as passing. The successful run uses the
existing optional Python environment and explicitly routes TEMP/TMP to E. No
assertion or skip policy was changed. These static gates do not replace the
pending complete native source regression.

## Native panic retention and ordered-frame controls

The initial host retained a model's normal failure receipt, but an unwinding
native panic could leave only a running receipt. Its long backtrace could push
the cause outside the captured final 512 stderr characters, after which temporary
cleanup removed the native log. A separate native hook now writes a bounded
first-panic record before calling the unchanged logger/backtrace hooks. Its final
stderr summary retains the cause and source location even when the record cannot
be written. The shared logger still force-captures a backtrace with `try_lock` and
preserves its original hook.

The host retains this bounded context before cleanup. An actual model failure
remains primary after an earlier caught panic; otherwise a later fatal footer
precedes the earlier caught observation. A successful child with an observed
caught panic still succeeds and emits Error. Cancellation keeps its original
result, with any separately observed panic recorded as a fault. No worker timeout,
restart, model retry or forced-panic product mode is added. This hook observes
unwinding panics; allocation aborts and native access violations remain distinct
failure classes.

The following gates ran against the recorded patch over committed source
`438a47ebc89d3bb87fd70d4515a0f8d51fc4b5bc`, before the subsequent main rebase.
The archive retains exact source hashes, actual executables' hashes, controllers,
default-level file logs, stderr, stdout and process exits.

| Actual gate | Result |
| --- | --- |
| Host protocol, bounded cause and priority controls | 11 passed; 217 other host tests filtered, none ignored |
| Original worker admission controls | All 31 passed with unchanged assertions |
| New production-hook test | Passed five real subprocess cases: fatal, unavailable record, caught, caught with unavailable record, caught then later fatal |
| Independently retained raw process outputs for those cases | Original exits 101/101/0/0/101; file Error and stderr Error visible at default Info; log output isolated from stdout; final 512 stderr characters retain cause and source location |
| Rebuilt worker and real Jobs | Both numerical outputs reproduce their earlier hashes; corrupted program, missing resource, live-upload cancellation and completed stale-result controls pass |
| Twelve ordered crops with CPU references and real WGSL Jobs | Every frame passes maximum 0.0002 / mean 0.00002 normalized error bounds; largest per-frame maximum is 0.000014858214 |
| Fixed backend-alternation observer | Maximum extra temporal error 0.000014215339; mean 0.000000184323; bounds remain maximum 0.0004 / mean 0.00004 |
| Deliberate 1% depth jump | Rejected by the same observer; no fitting or threshold adjustment |

The ordered inputs are authored crops of the existing credited NASA photograph,
not recorded video: frame `i` uses crop `[80+4*i,70+2*i,480,320]` and rational
PTS `i*1001/24000`. Both backends use the same Small model, resolution and
processing. One reference scale covers the complete temporal comparison. The
observer alternates already computed CPU/WGSL outputs every three frames; it does
not exercise a product scheduler, quality transition, hysteresis or export policy.
Each WGSL job still starts a cold native process. Costs are retained per frame;
these measurements establish neither interactive latency nor thermal stability.

Before applying this patch, the original full command finished with cargo exit
101 after 9,011.797 seconds. All 428 source-bound target invocations started, with
no missing or unexpected source targets. The 427 completed summaries report
4,978 passed, three failed, 78 original ignored and zero filtered tests. One
additional target crashed before its summary, so this is a failed full run:

- `format_version_boundaries`: project publication returned OS 5 / PermissionDenied
- `shell::project_precision_tests`: a real child failed to allocate 25,600,000
  bytes and exited `0xc0000409`
- `compositions::deep_library_draws_only_active_nodes_and_reuses_gpu_outputs_at_normal_resolution`:
  device creation returned `OutOfMemory`
- `exposure_sdr`: the first test began, then the target exited
  `0xc0000005 / STATUS_ACCESS_VIOLATION`; its complete summary is absent

The original assertions, ignores, features, GPU selection, stack and admission
budgets were retained. The original #88 plugin-settings child failure did not
reproduce in this run, which does not resolve its earlier intermittent cause.
Allocation failures and the access violation have no established common root
cause. Neighboring missing overlay-layer warnings are not that diagnosis.

The exact committed pre-patch head also completed CI with eight successes and two
policy skips. This records the old head only: it does not qualify the new patch
or runtime changes brought in by the subsequent main rebase. Complete regression
and exact-head CI of that final source remain pending, as do the other #75 gates.

The [panic, full-run and ordered-frame archive](data/depth75-panic-retention-v1.zip)
and [manifest](data/depth75-panic-retention-v1.json) were read back and every member
verified: 1,298,546 bytes, SHA-256
`6fb092c1e3cfc39fca416c63a61343152fe3be9e9c8f600fe94ec388047963b2`.
The host retains bounded panic cause/location, not a permanent complete native
backtrace after cleanup. Raw images, tensors, model resources, executables/PDBs
and optional runtimes are excluded from the public archive.

## Post-rebase complete run and loader isolation

The complete maintained-workspace command ran against unchanged source
`0646083d672b0d362901cc0a8c8992c41d59182c`, rebased onto maintained main
`afebb5cd3d3cfb668d68641ea665377d2fa0c546`:

```text
cargo test --workspace --exclude motion-android --features motion-desktop/ffmpeg,motion-host/ai-depth,motion-desktop-media/ai-depth --no-fail-fast --locked -- --test-threads=1
```

It finished after 4,547.844 seconds with Cargo exit 101 and controller exit 1.
All 430 source-bound invocations started, with no missing or unexpected targets.
Only 414 produced summaries: 3,141 passed, zero ordinary assertions failed,
four original ignores and zero filtered tests. Sixteen executables exited
`0xc0000135 / STATUS_DLL_NOT_FOUND` before running their tests. This is a failed
full regression. Its zero ordinary failures do not qualify the missing tests.
The earlier run's distinct assertion, allocation and access-violation failures
remain in their original source-bound evidence; no common cause is established.

The exact head above completed eight applicable CI checks with two policy skips.
That evidence does not qualify later source or replace local full feature coverage.

Two subsequent isolated loader controls preserve the original failures:

- Both affected test executables still fail `--list` with the original pinned
  runtime on PATH, while the pinned `ffmpeg.exe -version` succeeds. Individual
  library loading succeeds for avutil and fails for the other six libraries.
- A byte-identical test executable, hardlinked into a separate owned directory,
  fails listing without adjacent runtime libraries. Adding hardlinks to the exact
  seven pinned DLLs makes the same executable's listing exit zero. Original
  binaries, target directories, runtime installation, PATH, registry and
  permissions are unchanged.
- Fresh `LoadLibraryExW` controls for avcodec and swresample fail with default
  search (Windows error 126) and succeed with `LOAD_WITH_ALTERED_SEARCH_PATH`.
  This supports adjacent runtime placement as a working correction; it does
  not identify a particular missing transitive DLL or execute any test bodies.

The [failed-run and loader archive](data/depth75-loader-full-v1.zip) and
[manifest](data/depth75-loader-full-v1.json) retain the original complete logs,
metadata, controllers, terminal-session receipt, bounded environment
observations, library hashes and exact source snapshots. All 61 members were
read back and verified: 568,823 bytes, SHA-256
`e0e9fc7c4ce24f2356e937b668c117d2e5e69af2a0635af52e79d620a52cfa88`.
The source snapshots match the failed run's retained hashes, including the
three files subsequently edited for the observation below. No executable,
runtime library, model, image or tensor is included.

A complete run of the final integrated source, with its pinned runtime correctly
placed, is still required. The original 10 GiB free-space admission, feature set,
GPU selection, assertions, ignores and serial workload policy remain unchanged.

## Processing identity before a blocked operation

The separate explicit ONNX resolution work uses the same model ID/revision for
266, 392 and 518. Those fields alone cannot identify a hung job's actual profile.
Sample and video Info start records now also contain the requested processing
identity and complete model SHA-256 before the handler or video decoder runs.
Existing request/source context, in-flight publication, completion timing,
return values and stdout isolation remain unchanged. Disabled Info logging
does not allocate the digest string. No Present path is involved.

A new controlled-backend regression captures the default-Info log while each
of three requests remains blocked. All three share ID/revision but differ in
processing and hash; each snapshot must contain its exact requested identity.
The existing video decoder-blocking test has an additional identity assertion;
none of its previous assertions were changed.

These new assertions have not yet compiled or executed locally: free disk is
below the unchanged native-build admission. Scoped formatting/syntax and diff
checks are separate static evidence. Whole-file formatting already differs on
the original source, so no unrelated formatting rewrite was applied. The old
exact-head CI and archives above do not qualify this new patch. The additional
source-bound CI evidence below covers the two controlled assertions; complete
integrated native regression remains pending.

## Explicit-profile integration and inspected CI test bodies

The two blocked-start assertions actually passed in the desktop Linux job for
`2a08e053d1569306a9dd091db40e7eb1f264b2ca`:
[raw-job source](https://github.com/MAD-Producer/Kilonova/actions/runs/42224811554/job/127915508846).
The separate Rust/GPU job explicitly excludes `motion-host`, so its green status
does not prove these assertions. A collector initially expected both tests in
that excluded job; its failed receipt is retained alongside the corrected,
package-scoped verification. No product assertion or CI invocation changed.

The explicit-resolution change also has inspected passing test bodies on Linux,
Windows and macOS Intel, plus nine Python profile tests in Linux. Its complete
workflow passed before [PR #628](https://github.com/MAD-Producer/Kilonova/pull/628)
was merged with a Chinese commit message as
`5a025a2d23f2cc35ee4d306550861b086e0b2378`.
The [CI evidence archive](data/depth75-ci-tests-v1.zip) and
[manifest](data/depth75-ci-tests-v1.json) retain raw logs, exact source snapshots,
controllers and receipts, including a separate collector invocation that supplied
leaf names for fully qualified Rust unit tests. All 38 archive members were read
back and verified: 746,636 bytes, SHA-256
`8f55f0d40c01092268dc78ad11504ec34f83611745b6080e791c7b970360463d`.

The backend branch was then rebased onto that merged main commit. The
[integration audit](data/depth75-rebase-integration-v1.json) verifies ten exact
Git blobs and one fully inspected composition. The ONNX module matches the
qualified resolution source except for `capture` retaining its existing
`pub(super)` visibility, which the WGSL sibling already needs. The initial exact
blob check correctly rejected that difference; a source-bound full-run controller
must name the composed blob explicitly instead of disabling source checks.
Profile/export/test code, backend/job/video code, production logger and the
dependency lock otherwise remain exact qualified blobs. This is static source
evidence, not an integrated test pass. The final source needs its own CI and complete
workspace regression with the original feature set and runtime layout.

The prepared production-sink probe must additionally demonstrate that each Info
start reaches the actual log file while its controlled handler is still blocked,
without an explicit flush, and that the released failure reaches both file and
stderr while stdout remains isolated. That probe has not executed. These checks
do not establish real-model quality transitions, sustained performance, thermal
stability, capability selection or a frozen export policy.
