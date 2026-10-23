# Desktop performance study: accepted native data increment

Issue: #60. The issue remains open; the original scope is mapped below.

Measured source: `5824c882127397c562f731520e1c798316269938`, based on main
`9b35645ca66c5037e1e374207cde2d218e5d5e1c`. This is before #61's later
resource-policy delivery (`dc4fd44a`); these numbers are not measurements of that
later source. A documentation-only descendant publishes this record without
relabeling the measured commit or inferring an optimization benefit.

## Actual execution and raw archive

All six fixed workloads ran as a native Release executable on an NVIDIA GeForce
RTX 4070 Laptop GPU, Vulkan, NVIDIA driver 591.86. Every case retains 68 raw
frames: one cold, seven warmup and 60 measured. Total: 408 raw / 360 measured.
The nine focused archive/failure tests, full Python tool discovery (80 run,
one existing skip), both actual Release fixture tests, native study, archive
replay, provenance and tool inventory passed. No existing assertion or ignore
condition was relaxed.

The [raw archive](data/desktop-performance-5824c882.zip) contains all 30 original accepted files:
six raw reports, complete projects, final native PNGs, case events, actual
stdout/stderr, build/run start and exit receipts, summary and hash provenance.
It is 113763 bytes, SHA-256
`975e3ff4a374454a418a2cced2834bb03e534ccc031555089f1820e468a3128c`. No executable, user media or third-party artwork is included.
Generated geometry and YUV pixels are independently authored repository fixtures.
The [acceptance manifest](desktop-performance-baseline-acceptance.json) binds
exact commands, original logs, raw archive, source and binary hashes. Original
uncompressed data and the failed-run binary remain under artifacts/pr-queue on E:.

Extract the archive into a new directory, then verify its original data without
running a GPU or compiling. The exact publication ZIP was extracted to a new
directory and this replay passed with log SHA-256
`fe5af973f6c6c987670fa35f10f6cf69f29dfd1f3adf7f163efe7c36a50bc4e6`:

```powershell
python tools/collect_desktop_performance.py --output <extracted-directory>/accepted --validate-only
```

The executable's source stamp matches the measured commit. Independent PNG
decoding reproduced every native RGBA hash; the first five cases also preserve
exact project and final-pixel hashes from the earlier failed four-4K attempt.
All timed frames have real draws. Every four-4K frame has exactly four native
conversions, four conversion submissions and 49766400 uploaded YUV bytes.
Unknown timing remains unknown; this device supplied every requested timestamp.

## Results and measurement scope

All timings below are milliseconds, recomputed from the 60 measured raw frames.
Texture bytes are maximum observations after completed frames, deduplicated by
allocation identity. They are not a transient frame peak or physical residency.

| Workload | Waited wall P50 | P95 | P99 | Root/present GPU P50 | Observed texture bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| `static` | 0.474 | 0.638 | 1.087 | 0.038 | 10713840 |
| `animated` | 0.370 | 0.529 | 0.833 | 0.029 | 7373832 |
| `effect-chain` | 2.066 | 2.337 | 3.205 | 1.277 | 14759304 |
| `crossing-3d` | 0.237 | 0.389 | 0.678 | 0.023 | 7373832 |
| `nested-matte` | 0.403 | 0.495 | 0.645 | 0.032 | 11530633 |
| `four-4k-yuv` | 5.686 | 5.894 | 6.306 | 0.126 | 189850632 |

Waited wall includes scene preparation, explicit uploads and deliberate GPU
completion waits. The GPU column surrounds root composition and offscreen
presentation; earlier YUV conversion and nested/vector/MSAA preparation can be
outside that interval. It is not total end-to-end GPU work. The four-4K fixture
uploads/converts generated planes; it does not decode four video files.
Metadata enumeration and final PNG image readback are outside every timed frame.
Logical admission remains a separate instrument with an explicit raw-Presenter
uniform coverage gap. The environment was a development desktop with possible
background load; clocks, thermals, power and quiet-machine conditions were not
controlled. These values are neither native-window FPS nor a speedup claim.

## Fixed policy and retained failures

Every case uses 1280 x 720 at 30 fps, full-resolution encoding, the exact archived
project/effect parameters and a fixed 256 MiB source-texture allowance. The full
MemoryPolicy is stored in every report. Application defaults are unchanged.
The first failed attempt (`0b9da1fe`) compiled/passed both fixture tests but its
Release build loaded an older shared model artifact. Commit isolation removes
that timestamp assumption. The next actual run (`a381bbaa`) completed five cases
then refused four-4K uploads: required 136857604 versus budget 134217728 bytes.
That entire attempt remains failed; partial results are not counted as the
accepted six-case baseline.

The successful run explicitly reused a381bbaa's dependency directory only after
verifying that every tracked dependency input is identical. Only the benchmark
and its independent runner/tests/records may differ; native source, library
resources, Cargo manifests or lockfile changes refuse reuse. The new executable
still embeds `5824c882` and gets a new binary hash. This avoids rebuilding unchanged
dependencies without trusting old worktree timestamps. The earlier failed binary
was copied and SHA-256 verified before replacement. No payload, resolution,
source count or failure assertion was reduced to make the study pass.

## Original issue coverage

| Item | Disposition and remaining work |
| --- | --- |
| R60.1 Resource categories and ownership | Partial: actual texture references, allocation identities, unknown capacities and separate logical admission are recorded; CPU caches, external codec/driver allocations and inference remain unmeasured |
| R60.2 Logical/CPU/external distinction and aliases | Partial: shared identities are deduplicated and unknown stays unknown; the reusable synthetic CPU fixture is explicit, not a CPU decode-cache measurement |
| R60.3 Per-frame counters, timings and distributions | Partial: real main/YUV submissions, uploads, prepare, pipeline builds and raw percentiles are present; window deadlines and pressure-driven degradation reasons need Session-level studies |
| R60.4 Workload fixtures | Partial: six fixed desktop cases execute; real file decoding and mobile memory-pressure workloads remain |
| A60.1 Fixed conditions and raw publication | Accepted for this exact bounded study: source, project/effect parameters, quality, interval and all raw records are published; environment limits remain explicit |
| A60.2 PC/Android device matrix | Partial: this one recorded native PC GPU ran; no Android Adreno/Mali device, Android build or other desktop benchmark adapter is accepted |
| A60.3 Optimization claims | Retained as unverified hypotheses; no average-FPS, percentage saving or competitive-performance claim is inferred |

Android is unmaintained. #61's desktop delivery-scope decision is not used to
close #60's original physical-device checklist. Native-window/file-decoding,
complete CPU/external ownership, inference, physical pressure and before/after
optimization studies remain separate work. No UI panel, token, style, layout,
interaction or normal application frame loop is changed; DESIGN remediation
rows are retained.

## Actual source-bound gates

Focused tests overlap full suites; counts are not distinct totals.

| Gate | Result | Original log SHA-256 |
| --- | --- | --- |
| `tools` | Pass | `4f62009d8686c42f4a00f879d0b87756643147360c8de2d9ba40c1929bf8a4eb` |
| `native` | Pass | `db7b6a7c87ab38164e834e927d6a4898f9533060a295982e87a527f2d2ac44c5` |
| `fixtures` | Pass | `34afe22d8937435b0327bcbd7e8ddec6100f4e8c4035982150903285247038e8` |
| `replay` | Pass | `fe5af973f6c6c987670fa35f10f6cf69f29dfd1f3adf7f163efe7c36a50bc4e6` |
| `provenance` | Pass | `144a6a2b31dde8d226b8cc4550a10dee9b6ac2a90ffc882c72d4d7bcc7dcf458` |
| `inventory` | Pass | `d8603ba5209d8b86a2199e080b90da8ff71f2d059e24ade0eeaaadc24636f905` |
