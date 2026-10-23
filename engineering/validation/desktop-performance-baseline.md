# Desktop GPU baseline: reproducible native study

Issue: #60. This increment does not close the issue.

Status: implementation and archive-validation regressions are present. The six
focused Python tests passed, including an actual failing child process and
optimized Python rejection. Complete tool discovery also passed: 77 tests run,
one existing skip. The initial full discovery failed because the local Python
lacked PyYAML; using the existing pinned installation fixed the environment,
without changing tests or adding a dependency. The original failed attempt is
retained. The original `0b9da1fe` source compiled and passed both Rust fixture
tests. Its Release build failed because the shared target reused an older model
artifact: the compiler saw `Error::Expression.message` while that clean source
defines `source`. Original logs and the failed receipt remain archived. This
successor isolates builds by complete commit; its updated tests and actual
native runs remain pending. No failed attempt is relabeled as a performance run.
Protocol fixtures in Python tests are not performance evidence.

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
gets a new renderer. The first frame is retained as cold, the next seven as
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

Native adapter, driver, backend and device type are stored in each report.
The wrapper records host platform, CPU count and an environment note. It does
not establish controlled temperature, power state or freedom from background
load. Results from a loaded machine remain measurements of that environment.
There is no fixed FPS gate or unsupported speedup claim.

## Remaining issue scope

- Actual Release runs and native fixture acceptance are still pending here
- The existing Session/desktop paced-playback study remains separate; this
  example does not measure a native window, UI, physical input or display deadlines
- Actual file decoding, CPU image/cache ownership, inference weights/activations,
  external driver/codec allocations and physical memory-pressure studies remain
  outside this increment
- Android is unmaintained. No Adreno/Mali device or Android build is accepted;
  #61's desktop delivery-scope decision is not used to close #60's original
  physical-device checklist
- Earlier static loop-work/scratch reports and performance hypotheses retain
  their original scope; no average FPS or desktop run proves those hypotheses

DESIGN remediation rows are retained because no panel, token, layout, style or
interaction file is edited by this increment.
