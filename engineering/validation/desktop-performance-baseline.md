# Desktop GPU baseline: reproducible native study

Issue: #60. This increment does not close the issue.

Status: the desktop native-data increment is accepted on source `5824c882`.
All six actual Release workloads, 408 raw / 360 measured frames, independent
native-pixel hashes, both Release fixture tests, nine focused archive tests,
complete Python tool discovery (80 run, one existing skip), archive replay,
provenance and inventory passed. See the [completed record](desktop-performance-baseline-completed.md),
[exact acceptance manifest](desktop-performance-baseline-acceptance.json) and
[published raw archive](data/desktop-performance-5824c882.zip). The two original
failed attempts remain failed. Protocol fixtures alone are not performance evidence.
Issue #60 remains open for its remaining original scope.

The current collector writes schema 3, adding original encoder words and exact
request/command identities. See the [accepted command study](gpu-command-release-completed.md)
and [maintained-scope audit](issue60-desktop-scope-completed.md).
The earlier schema-2 increment additionally carries separate
CPU package archive/expanded-file/text ownership, independent program shader
text, and the two actual Vec capacities of the single reusable YUV fixture.
The published `5824c882` schema-1 study remains attributed to its original source;
replay it with that source's collector, not the schema-2 collector. No compatibility
fallback invents missing observations. The new source `709de340` is accepted on six actual Release cases, 408 raw /
360 measured frames, independent pixels and replay. See the
[CPU cost-report completion](desktop-performance-cpu-completed.md) and its
[exact receipts](desktop-performance-cpu-acceptance.json). Both historical and
current studies keep their original measured source and remaining limits.

## Run and preserve evidence

Commit the study source, select an otherwise idle desktop and a new output
directory. Use the project's normal native build environment. The command builds
the Release example with the current commit embedded into the binary, records
the binary hash and runs the exact six fixtures:

```powershell
python tools/collect_desktop_performance.py --output E:/Dev/aem/artifacts/desktop-baseline-01 --target-dir E:/Dev/aem/target/desktop-baseline --samples 60
python tools/collect_desktop_performance.py --output E:/Dev/aem/artifacts/desktop-baseline-01 --validate-only
```

The tool refuses a dirty source or an existing output directory. Build outputs
use `<target-dir>/desktop-performance/<complete-commit>/release`, isolating
workspace artifacts from other revisions even when they share a cache root.
An explicit `--reuse-dependencies-from <complete-ancestor-commit>` may reuse a
previous directory only when every tracked dependency input is unchanged. The
allowlist contains only this example and its runner/tests/records; any native
source, Cargo manifest, lockfile or library-resource change refuses reuse. The
new source revision is still embedded in the example, verified at runtime and
archived separately from the cache revision. This avoids rebuilding unchanged
dependencies when correcting a study condition, without trusting timestamps
across worktrees.
The two fast Rust fixture/statistics tests run in normal Cargo test discovery;
the actual GPU study remains explicit. The archive validator also rejects missing
draws or any frame lacking all four native 4K YUV uploads/conversions in that case.
It preserves
build/native stdout and stderr, start and exit receipts, the original failure,
per-case events, full authored project JSON, all raw frames and a final PNG.
Source changes during the run fail acceptance. Successful archives record file
hashes; replay checks those hashes, project identity and distributions. No normal
application/MCP stdout or UI behavior is changed. No dependency or third-party
asset is added; all fixture geometry and YUV pixels are independently authored.

The native example can also run directly, but the wrapper is required for
source/binary provenance. Do not interpret an example launched without an
embedded commit as a source-bound baseline.

## Fixed workloads

All projects use 1280 x 720, 30 fps and an explicit frame interval. Every case
gets a new renderer with a fixed 256 MiB source-texture allowance; four full-size
RGBA outputs and their YUV planes cannot fit the normal 128 MiB allowance. This
changes the explicit study configuration, not the application's defaults. The
first frame is retained as cold, the next seven as
warmup; only the requested following frames contribute to P50/P95/P99/max.
The serialized project and its hash pin effect versions, parameters and keys.
Cold means the first frame of a fresh application renderer, not a purged driver
or operating-system shader cache. Steady samples retain actual pipeline-build
counts rather than assuming every pipeline has become warm.

| Case | Actual exercised workload |
| --- | --- |
| `static` | Two overlapping translucent solids and two vector ellipses sampled repeatedly at frame 0 |
| `animated` | Four independently keyed positions across the fixed interval |
| `effect-chain` | Eight nontrivial effects on one layer, including blur and directional blur |
| `crossing-3d` | Four animated, rotated spatial planes |
| `nested-matte` | A nested composition containing an alpha track matte |
| `four-4k-yuv` | Four distinct native 3840 x 2160 YUV plane uploads/conversions each frame |

The YUV workload uses one reusable CPU fixture. It does not decode four video
files or exercise a codec cache. Plane contents, the changing-pixel formula and
source dimensions are recorded separately from the solid placeholders in the
authored project. The fixture must not be presented as end-to-end video playback.

## Instruments and limits

Raw records retain CPU scene sampling, renderer prepare, upload wall time,
encode/submit, deliberate completion wait, waited wall time, GPU timestamp
intervals, submissions, conversions, image/video byte counters, vector upload
capacity/counters and pipeline builds. Submission counters describe explicit main
and YUV conversion submissions; implicit driver/upload staging is not counted.
Known texture capacities are deduplicated by allocation identity; multiple
ownership references do not add capacity. Unknown capacities remain unknown.
The logical admission observation is a separate instrument and is not equated
with the texture subtotal or physical residency. It is sampled after a completed
frame, not reported as an observed transient frame peak.
This standalone Presenter's two raw uniform buffers are outside that admission
instrument; the report keeps this explicit coverage gap.

GPU timestamps surround the renderer's root composition interval and offscreen
presentation. Earlier YUV conversion, nested/vector/MSAA preparation can fall
outside those timestamps. Waited wall time includes the complete submitted work.
Unsupported timestamps remain null; an enabled timer missing its actual sample
fails the run. Image readback is excluded from every timed frame. One final PNG
is read afterward. Neither the deliberate per-frame wait nor per-frame metadata
enumeration is enabled in normal application playback.

CPU `knownPayloadBytes` and `knownCapacityBytes` are observed owned buffers,
separate from textures and logical admission. Actual Arc identity deduplicates
shared packages and program shaders; independently allocated equal text counts
separately. Archive, expanded files and package-owned text are distinct buffers,
as are retained program shader copies. Four YUV uploads borrow one generated
frame, so the CPU inventory counts its Y and UV Vec capacities once. No decoder
cache is inferred from these pixels. Unknown metadata, containers, external
owners and process/driver residency remain unknown, never zero. Queries retain
no owners. Raw records preserve bounded allocation entries and their references;
the summary adds distributions of the observed CPU payload and capacity.

`metadataSampleUs` measures the separate texture/CPU enumeration and admission
query after the frame has completed. It is excluded from `waitedWallUs` and has
its own distribution. JSON serialization, retained report objects and final PNG
scratch remain outside that timing and the observed CPU subtotal. Enumeration
can perturb later samples; it is a diagnostic study condition, not a claim of
zero instrumentation overhead or application frame throughput.

Native adapter, driver, backend and device type are stored in each report.
The wrapper records host platform, CPU count and an environment note. It does
not establish controlled temperature, power state or freedom from background
load. Results from a loaded machine remain measurements of that environment.
There is no fixed FPS gate or unsupported speedup claim.

## Historical study boundaries

- This exact six-case desktop Release study is accepted; later source, other
  adapters and before/after claims require independent runs
- The existing Session/desktop paced-playback study remains separate; this
  example does not measure a native window, UI, physical input or display deadlines
- Actual file decoding, CPU image/cache ownership, inference weights/activations,
  external driver/codec allocations and physical memory-pressure studies remain
  outside this increment
- Android is unmaintained. The user explicitly excluded Android validation
  from #60 on 2026-10-23; no Adreno/Mali device or Android build is accepted
- Earlier static loop-work/scratch reports and performance hypotheses retain
  their original scope; no average FPS or desktop run proves those hypotheses

DESIGN remediation rows are retained because no panel, token, layout, style or
interaction file is edited by this increment.
