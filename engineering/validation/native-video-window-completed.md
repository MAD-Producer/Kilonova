# Native file/window baseline increment for issue #60

This increment provides a reproducible desktop baseline, native-window pixel
evidence and completed-frame activity counters. It does not complete issue #60.
The user explicitly excluded Android validation on 2026-10-23; Android is neither
claimed nor a remaining acceptance blocker.

## Source and evidence identity

| Role | Source or artifact |
| --- | --- |
| Actual Release/native execution | `df046e09ae2ca51298bbfc4235f60f5a518bdaa7` |
| Corrected strict primary-report validator and interval tool | `acf1c3b222a2b7908ae4c32361b76109244c6316` |
| Latest-main delivery implementation | `1092420cb189ec0c0d7fae7103d50d83a6c56175` |
| Evidence archive | [native-video-window-df046e09.zip](data/native-video-window-df046e09.zip) |
| Archive SHA-256 | `6b10768aeaee3e952a1c181d3c464afa31ffef432063fe29b7dd84c39549758f` |
| Archive length / evidence entries | 766561 bytes / 193 evidence entries plus inventory |

The archive includes per-entry lengths/hashes, actual process receipts, original
primary and secondary reports, raw completions, screenshots, fixture/tool
metadata, negative evidence, a measured-source patch and a verified Git bundle.
Full video/reference rasters, external runtime DLLs and executables are excluded;
their retained hashes, dimensions, commands and local paths remain in the
original receipts. The archive is a record of an actual run, not a portable claim
that unavailable executable/source inputs can be silently substituted.

Measured source, validation source and delivery source are intentionally separate.
The measured implementation is reconstructible from the bundle. Thirteen owned
delivery source blobs match the validated implementation; the inventory retains
latest-main entries. The byte-preserving Rust report copy is compiled by the exact ffmpeg
desktop suite described below. No later native performance rerun is claimed. No later implementation is labeled as measured.

## Actual native observations

The original Release build exited 0, and the exact ignored native test actually
ran: one passed, zero failed and zero ignored. Its two visible native windows
used independent real file decoders on the selected Windows desktop. Each source
is 3840 x 2160, 30 fps, 600 frames. Composition is 1280 x 720; the native surface
is 1280 x 800. High preview and a 256 MiB source-texture allowance remained
unchanged. Actual full-resolution decoded frames and all video center pixels
were checked against independently decoded file references.

| Observation | One source | Four sources |
| --- | ---: | ---: |
| Completed / associated root GPU samples | 118 / 118 | 26 / 26 |
| Cold / warmup / measured completions | 1 / 30 / 87 | 1 / 8 / 17 |
| Measured seek/render cycle P50, microseconds | 20886 | 142584 |
| Measured seek/render cycle P95, microseconds | 25034 | 252746 |
| Measured seek/render cycle P99, microseconds | 29414 | 252746 |
| Requested-source deadline misses | 57 | 17 |
| Skipped source buckets | 3 | 74 |
| Median observed instrumented submissions per measured interval | 1 | 7 |
| Median video conversions per measured interval | 1 | 4 |
| Median video upload payload bytes per measured interval | 12441600 | 49766400 |

The four-source result is a slow baseline. These workloads are different source
counts, not an optimization comparison. Requested-source deadlines compare the
nominal source boundary with a native reply, not physical scanout FPS.

## Corrections and preserved failures

The real native test passed, but the original outer collector failed strict
identity validation. Parsing and serializing a secondary JSON copy moved 20
fractional frame IDs by one ULP. Every original host report frame still exactly
matched its raw completion. The corrected validator reads that primary report;
all original frame, GPU count, coverage and distribution equalities passed.
There is no rounding tolerance or rewritten evidence. The original collector
failure remains failed; independent corrected validation has a separate receipt.
The delivery driver now retains original profile bytes without reserialization.

The recorder previously treated native sequence offsets as record-vector indices.
Chrome-only presents during decoder polling create gaps, so associated callbacks
were silently lost. Matching now uses actual sequence identities. The original
source failed two controlled gap tests; the fixed source passed all four tests
after an actual rebuild. An initially reused old executable is retained and is
not counted as fixed-source acceptance.

An earlier complete Debug render/host regression aborted in an existing
`frame_blend` test with a non-unwinding `NonNull::offset_from_unsigned` panic.
Its original log and bounded diagnostics are retained. The Windows abort leaf
does not establish the causal stack, and isolated non-reproductions do not prove
a fix. One complete full-backtrace diagnostic at `98e84762` exited 0 with 667
passed, zero failed and one existing ignored test in 2218.43 seconds. It did not
reproduce the abort and does not establish its cause. The separate exact delivery-source
suites exited 0 with 1498 passed, zero failed and 3 ignored tests. The original abort is
not waived.

Earlier zero-associated-GPU and fixture-center failures also remain attributed to
their original binaries/sources. No existing assertion, product retry or backend
policy was changed to make acceptance pass.

## Practical limits and remaining work

Root GPU composition/presentation timing excludes earlier conversion, nested,
vector and MSAA work. Counter intervals include intervening decoder polls and
chrome work. Overlapping conversion counts cannot be summed. First, missing and
decreased counter intervals stay unknown. Upload payloads are not bus traffic;
pipeline counters exclude other startup/driver compilation. Timestamp-return
payload bytes do not constitute complete readback coverage.

CPU/texture ledgers observe application ownership, not complete process RAM,
external decoder allocation or physical VRAM residency. Native source-budget
pressure/recovery, remaining GPU stages and readback accounting are separate
increments. No controlled optimization gain is claimed. UI, panel styling and
interaction behavior are unchanged.

## Original issue dispositions

This table combines existing published coverage with this bounded native study.
A documented boundary is not a repaired defect or full issue acceptance.
Six referenced historical ZIPs passed a fresh whole-archive SHA-256/CRC recheck;
131 inventoried files also matched their retained lengths/hashes. The validation
appendix retains this integrity report. The older session record has no published
ZIP and is not included in that ZIP recheck. No historical GPU study or test suite
was rerun by this integrity check.

| Item | Evidence available | Remaining boundary |
| --- | --- | --- |
| R60.1 Resource categories and ownership | Existing [session](session-resource-memory-completed.md), [media CPU](media-cpu-memory-completed.md), [effect packages](effect-package-memory-completed.md), [original source pixels](original-source-memory-completed.md) and [image decoding](image-decode-memory-completed.md) records; this study adds actual native completion snapshots | Driver/codec external allocation, uninstrumented container overhead and inference capacity remain unknown; no total-process-RAM or physical-VRAM claim |
| R60.2 Logical/CPU/external separation and aliases | Existing ownership records distinguish retained allocations/references and logical admission; this study preserves those distinct scopes at successful completions | Sequential snapshots are not instantaneous cross-store peaks or external residency measurements |
| R60.3 Per-frame activity, timings and distributions | All 144 actual completions retain source/sequence identities, CPU/native costs, root GPU samples and raw resources; completed-frame intervals expose submits, conversions, upload payload and pipeline builds with P50/P95/P99 and unknown counts | Conversion/nested/vector/MSAA GPU stages and complete image/timestamp readback activity are separate instrumentation/acceptance increments |
| R60.4 Reproducible workload projects/scripts | [Six-case headless study](desktop-performance-baseline-completed.md) covers fixed static/animated, effects, crossing 3D and nested/matte cases; this increment adds authored four-file fixtures and actual one/four-file native windows | Actual native application-budget pressure/recovery remains a separate pending study; this baseline is not physical driver/VRAM exhaustion |
| A60.1 Fixed conditions and raw publication | Each study retains its original resolution, effects, quality, interval, source, raw records and failures; later delivery tests are recorded separately | Background load/power/thermal conditions were not controlled; results are not optimization comparisons or scanout FPS |
| A60.2 Device identity and Android scope | Original native metadata records this Windows adapter, driver and API | Historical Android/Adreno/Mali pending statements are superseded by the user's explicit 2026-10-23 exclusion, also recorded in issue #60; no Android result is claimed |
| A60.3 Benefit claims | Percentage savings, speedup and competitive-performance statements remain unverified hypotheses | No benefit is inferred from these different workloads or from output-invariance checks |

## Delivery validation

Exact delivery source `1092420cb189ec0c0d7fae7103d50d83a6c56175` passed provenance, inventory, full Python
tool discovery and whitespace gates. These records are independent of the
measured df046e09 performance evidence. No existing assertion was relaxed.

| Gate | Actual result | Original log SHA-256 |
| --- | --- | --- |
| `defaultCheck` | Exit 0 | `b1249652a77f77f68bdadcd0d6c9230b1f2f297413467fa865914da849bfdc5e` |
| `regressions` | 4 passed / 0 failed / 0 ignored | `f51f8b9bc64fc72075b04e6720cc63c929470fc2e4da088f876b628f8be7820e` |
| `completeSuites` | 1498 passed / 0 failed / 3 ignored | `4c3fa1ff018ccc393329323c2097e16f797bba991a88c40fad192e1dc0aa7520` |

Focused tests overlap the complete suite; their counts are not distinct totals.
The explicit native study is ignored by default and was actually run separately
at the measured source. It is not counted as a default-suite performance pass.

[Delivery acceptance](native-video-window-delivery-acceptance.json) binds exact
commands, source, actual terminal receipts and all test summaries.
[Original validation logs](data/native-video-window-delivery-1092420c.zip)
has SHA-256 `290eb576a371c94888520cd49e6f0d1f9268b926f040f28c2d1c83eb17afb6cc` and includes the separate
full-backtrace diagnostic (667 passed / 0 failed / 1 ignored),
which did not reproduce or establish the cause of the original abort.
