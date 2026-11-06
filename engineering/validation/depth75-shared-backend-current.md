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

## Source-bound execution

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
| Final host protocol/ownership/failure controls | 6 passed; other 217 host tests filtered, none ignored |
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
