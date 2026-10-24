# Issue 60: maintained desktop requirement audit

This document maps the original four implementation and three acceptance items
in [issue #60](https://github.com/MAD-Producer/Kilonova/issues/60). It is not a
claim of complete process-memory or GPU-stage coverage. Android validation is
excluded by the user's explicit scope, also recorded in the issue on 2026-10-23.

The new command-observation code and native measurements use source
`ff52fbd2abd370b1d72fa6c86231cf8f4d24fa21`. Previously published increments
retain their own measured/tested commits below. A final documentation-only
delivery commit must not relabel these runs as tests of its new commit hash.

## Original requirements

| ID | Evidence | Scope and disposition |
| --- | --- | --- |
| R1: resource categories and ownership | Existing texture/allocation ledgers distinguish images, YUV planes/conversion outputs, effect scratch/resource textures, vector/MSAA, masks, snapshot/composition and final-frame cache; ownership IDs deduplicate aliases. [Package/program buffers](effect-package-memory-completed.md), [source pixels](original-source-memory-completed.md) and [glyph/mixer buffers](media-cpu-memory-completed.md) add source-bound CPU ownership evidence | Supported current producers are observed; unknown owners/containers are explicit exclusions. No built-in inference-weight/activation allocator exists in this source. That named domain remains excluded/unknown, not measured zero or a completed inference feature. This audit adds no inference engine |
| R2: logical texture / CPU cache / external memory | `Session::resource_memory_ledger()` separates actual CPU payload/backing capacity, deduplicated texture capacity and logical admission. Driver residency, decoder-internal storage and process residency are null. Shared CPU Arc identities and texture allocation identities count once | The original distinction is delivered. These are neither physical VRAM/RSS nor an atomic cross-domain/frame-peak total. Unknown external storage is intentionally unknown |
| R3: per-frame cost records | [Original native baseline](desktop-performance-baseline-completed.md), [CPU census](desktop-performance-cpu-completed.md), [readback observations](readback-cost-observations-completed.md) and the current command records retain submit/conversion/upload/readback counts, CPU prepare, root GPU and encoder intervals, cold/warm samples and P50/P95/P99. [Native pressure](native-video-pressure-completed.md) retains actual refusal/degradation/recovery reasons | Encoder/root intervals can overlap and are not additive exclusive time. Queue writes before an encoder, arbitrary independent callers and raw uninstrumented submissions remain outside command intervals. A universal profiler is not an extra requirement |
| R4: reproducible scene families | Six fixed 1280 x 720, 30 fps projects exercise static, animation, eight-effect chain, crossing 3D, nested/matte and four 3840 x 2160 YUV conversions. Separate [native-window](native-video-window-completed.md) and [application-budget](native-video-pressure-completed.md) studies exercise four independently encoded video sources | Generated YUV is not file decoding. Pressure is the actual desktop application source-texture allowance/refusal/recovery, not physical OS/driver exhaustion. No additional physical-exhaustion or mobile study is claimed |
| A1: fixed conditions, source and original records | Original archives bind full source/binary hashes, authored projects/parameters, full-resolution quality, authored intervals, raw cold/warm/steady frames, original channels and phase receipts. New command records retain original native query words and exact identities | Protocol fixtures and Debug samples are not Release performance evidence. The [current schema-3 Release study](gpu-command-release-completed.md) retains its actual 408 raw / 360 measured frames and archive identity |
| A2: GPU/driver/API | Actual native reports name NVIDIA GeForce RTX 4070 Laptop GPU, Vulkan and NVIDIA 591.86. Each archive retains its own device metadata | This accepts the observed desktop/device scope, not all desktop GPUs or backends. Android/Adreno/Mali validation is explicitly excluded; no Android acceptance is claimed |
| A3: unsupported optimization claims | All records distinguish observations from hypotheses; no controlled acceleration or saving is inferred | “128 to 64 MiB”, “38%”, “5–20x” and parity claims remain unmeasured hypotheses; 128 to 64 is mathematically 50% |

A documented exclusion is not a repaired defect. In particular, inference
production, external physical residency, physical OS exhaustion, exclusive GPU
busy time and display scanout are not newly implemented or measured by this
delivery. Those boundaries do not become new implicit feature work under #60.

## Source-bound acceptance

The [command correctness completion](gpu-command-observations-completed.md)
binds the actual ff52 full Desktop 893/0/3 result, both checks, focused 50 Rust
tests, 17 Python tests, 15 affirmative native reports and six Debug workloads.
Their overlapping counts must not be added. Earlier full Render/Host 703/0/1
belongs to `11c342d3`; its failed full Desktop attempt remains failed. ff52's
later pass does not establish the earlier native crash's cause or a fix.

The native application-budget study belongs to `53f2b24e`, with 127 completed
source frames and 127 associated root GPU samples. It actually lowers the
allowance from 256 to 32 MiB, observes refusal and level-1 640 x 360 output,
then restores 256 MiB and waits the production warm/cooldown conditions before
recovering level 0 at 1280 x 720. Original retained fronts and four decoded
reference centers are checked. It is not a performance improvement comparison.

The earlier six-case Release archives belong to `5824c882` and `709de340`,
each with 408 raw / 360 measured frames. They are not reruns of ff52. All
current command instrumentation adds explicit marker/readback overhead and
changes the measurement conditions; historical timings are not used as a
controlled before/after comparison.

## Delivery boundary

The current schema-3 Release collection and independent replay/pixel/archive
validation are accepted; see its completion record above. Final-head required
CI remains pending before declaring the maintained scope delivered. No local
passing subset, table disposition or archive checksum alone closes the issue.

No UI/panel/layout/style/interaction changes, new dependency or third-party
asset are part of this increment. Existing DESIGN remediation rows are retained
and none is claimed fixed. All measurement geometry/pixels are independently
authored. No Adobe binary or product resource is inspected or redistributed.
