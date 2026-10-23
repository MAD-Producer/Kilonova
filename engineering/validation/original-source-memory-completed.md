# Source pixel cache CPU observation acceptance

Issue: #60, partial implementation. The issue remains open.

## Delivered behavior

The explicit Session `resource_memory` query uses one bounded identity collector
for active playback video and original-source image/video caches. Shared Arc
allocations count once across both collections; independent equal buffers count
separately. Actual Vec lengths and capacities are separate. YUV counts both
planes without an RGBA conversion. Temporary Arc clones prevent identity reuse
while inspecting caches and are released before returning the query.

The observation response is version 2. `cpu.sourcePixelCaches` replaces the
active-only `cpu.videoDecodedCache` field. This deliberate observation-schema
change avoids presenting overlapping independent reports as an additive total.
The existing standalone `VideoFrames::cache_memory_ledger()` contract remains.
Normal snapshots, playback, cache admission and decode results remain unchanged.

The query does not call reader `request`, `poll` or `current`, consume queued
results, open source files, initialize graphics or modify project/history/state.
The original image cache is observed even after the source file has moved.
Cancelling the reader removes its cache ownership; escaped input packets can
remain valid and are explicitly outside the observed subtotal. This does not
claim their process memory was freed.

Each stream collection visits at most 1024 entries. The common collector keeps
4096 allocation identities and eight contexts per allocation. Additional streams,
unobserved allocation references and omitted contexts are counted. Query-local
limits reset on the next read. Stream-cache contention uses `try_lock`: busy
caches have unknown/null byte values rather than zero. Poisoned caches preserve
object/source/phase in the original error. No logging occurs under these locks.
Sampling multiple caches is explicitly non-atomic.

## Actual source-bound verification

| Item | Actual result |
| --- | --- |
| Complete unfiltered motion-host suite | Source `ff31e908c523ad6ef5d2fb0c37350ea3d3679dd3`: 141 passed, zero failed, zero ignored |
| New ownership regressions | All seven listed in the acceptance JSON actually executed successfully |
| Existing coverage | Original four video-cache memory regressions and Session public-query/package-owner integration tests also executed |
| Provenance and whitespace | Passed on the same source; no dependency, asset, panel, token, layout or interaction change |

The PNG regression writes an independently authored four-by-four PNG, uses the
real asynchronous image decoder and public Session plugin bridge, then renames
the source before reading the report. Exact payload/capacity, unchanged pixels,
Arc owners, project/revisions/history/texture ledger and repeated queries are
checked before and after cancellation. The escaped packet remains readable.

The original-video regression uses the actual async cache/worker machinery with
the repository's authored YUV decoder stub. Its 4-by-2 YUV420 planes contain
eight and four real bytes, retain rotation metadata, and enter Session totals
without conversion or source IO. It is not a real video-codec, file-decoding,
throughput or GPU performance acceptance. The source file is deliberately an
authored non-media fixture, not mislabeled external video.

The remaining regressions construct actual shared and independent allocations,
spare capacity, two stream collections, controlled cache contention/recovery,
4097 image identities and 1025 stream entries. They verify bounds, release of
each owner and retained unknowns. Bounds fixtures do not imply video playback.

One existing Session test changes its expected field to `sourcePixelCaches` and
adds an exact schema-version assertion. Its original exact CPU totals, scratch
payload/capacity, clear/release and public-bridge assertions remain. No assertion
is weakened or removed, and no ignore is added.

The first complete attempt on b273180a failed in the new video fixture before
its observations: its one-frame source was assigned the default two-second
layer range. The successor adds only a one-frame `LayerTimeline` and explanatory
comment (seven added lines). No assertion or production code changed in that
fix. The original failing receipt/log is archived, and the entire unfiltered
suite was rerun on the successor source rather than only rerunning that test.

The source was frozen throughout each run. Build/temporary files stayed on E:,
with one owned compiler. The first target was fresh; the successor reused its
dependency cache with Cargo source fingerprints and incremental disabled.
These correctness results are not relabeled to a later delivery/documentation
commit. Delivery must record the exact blob identity on its current main base.

## Delivery source identity

The publication branch starts at actual main `741a1c738ca48fbbb0468540d090e074da0591d7`. Its code commit
`ee5a5df876979fa15ca8015b0bc4b277f0b2b79b` contains only this fourteen-file increment. Every implementation,
test and API-document blob is identical to the measured source ff31e908, as
recorded in the acceptance JSON. Its two private implementation/fixture commits
are applied as one clean increment; the private pending-PR ancestry is not
published. The original full-host result remains attributed to ff31e908. Current
publication-head CI is required separately before merge.

## Original evidence

[Source-bound commands and receipts](original-source-memory-acceptance.json)
record every terminal exit and log digest. [Original logs/receipts](data/original-source-memory-ff31e908.zip)
contain thirteen original files, including the initial failure, plus an internal
per-file inventory. Archive SHA-256:
`38ad06a387cf9b3f81990c1221c7f612642cadff3c161f8d1a5699c382c4e4fb`. No build binary or third-party media is included.

## Issue #60 dispositions and limits

| Item | Disposition |
| --- | --- |
| R60.1 cached source pixels | New actual-capacity observation across retained playback/original image/video caches |
| R60.2 shared ownership | New common identity scope, independent-equal-buffer and owner-release regressions |
| Worker/queued completion pixels | Pending, explicitly unknown/null; busy or cancel does not prove zero allocation |
| Codec internals, retiring workers, escaped packets and containers | Outside subtotal; no process RSS or physical VRAM claim |
| R60.3 native per-frame reporting | Earlier source709de340 study retains its source/scope; its standalone renderer fixture has no original-input worker and is not retroactively reaccepted here |
| R60.4/A60.1 file/window/pressure workloads | Still pending beyond earlier accepted six desktop headless cases |
| A60.2 physical Adreno/Mali and inference | Still pending; #61's desktop-only exception does not close #60 |
| A60.3 before/after gains | No speedup or residency reduction claimed |

DESIGN remediation rows are retained because no panel/visual/interaction file
is edited. This completion document accepts only the observations above.
