# Native cost report CPU ownership acceptance

Issue: #60, partial implementation. The full issue remains open.

## Result and fixed scope

Measured source `709de340a6e6f660d1aba4e8532f4a1c36912626` completed all six actual Release workloads:
**408 raw frames, 360 measured frames**, with a fresh source-isolated native
build. The source/binary hashes, actual adapter/driver/backend, authored projects,
raw schema-2 records, cold/warmup samples, final PNGs, original output channels
and terminal phase receipts are preserved. No previous binary or dependency
cache from another native source was reused.

Every frame separates GPU texture capacities, logical allocation admission and
CPU owned-buffer lengths/capacities. CPU domains are package archives/expanded
files/package-owned shader text, independently retained program shader text,
and the generated YUV fixture's two actual Vec buffers. Actual Arc identity
deduplicates aliases within each owned allocation domain. Unknown metadata,
containers, codecs and external/native allocations stay null, never zero.

Four 3840 x 2160 YUV uploads borrow **one** reusable CPU fixture: its payload is
12,441,600 bytes once, while each frame's four real uploads remain 49,766,400
bytes with four conversions/submissions. The fixture is generated data, not
four file decoders. Other cases have no generated CPU YUV owner.

Enumeration occurs after frame completion. `metadataSampleUs` measures the
texture/CPU/admission queries outside `waitedWallUs`; both keep distributions.
It excludes serialization, retained report JSON and final readback/PNG scratch.
This census can perturb subsequent samples and is an explicit diagnostic-study
condition, not application frame throughput or zero-overhead instrumentation.

| Case | Final observed CPU capacity, bytes | Packages / program shader allocations | Metadata p50 / p95, microseconds |
| --- | ---: | ---: | ---: |
| static | 15999060 | 7 / 3 | 103.0 / 167.0 |
| animated | 15999060 | 7 / 3 | 61.0 / 147.0 |
| effect-chain | 16108767 | 7 / 9 | 60.0 / 106.0 |
| crossing-3d | 15999060 | 7 / 3 | 29.0 / 47.0 |
| nested-matte | 15999060 | 7 / 3 | 37.0 / 62.0 |
| four-4k-yuv | 28440660 | 7 / 3 | 113.0 / 184.0 |

The observations are after completed frames, not transient memory peaks,
process RSS or physical VRAM. Complete coverage remains false.

## Actual checks

- Full Python tool discovery: 83 tests run, one pre-existing skip; no failures
- All original nine archive/failure checks retained and three CPU/schema checks
  added; protocol fixtures are not GPU performance evidence
- Both Release example fixture/statistics tests executed and passed
- New-source six-case native execution, original stdout/stderr/start/exit logs,
  clean source/binary checks, all file hashes and archive replay passed
- CPU domain/aggregate subtotals, bounded identities/contexts, unknown coverage,
  single YUV allocation and actual upload/conversion workload verified
- Independent PNG decoding verified each report's RGBA hash, size and nonempty
  pixels. All six project and independently decoded final-pixel hashes match the original `5824c882` study. This is an output-invariance check, not a controlled performance comparison.
- Provenance, validation inventory and diff checks passed

The earlier CPU correctness acceptance remains separately attributed to source
`348d9672` (full effects/render559/0/1existingIgnored) and source `1a2b4bf1`
(full media/host280/0/2existingIgnored plus focused4effects+1render). See the
[ownership completion record](effect-package-memory-completed.md). Those counts
are not relabeled to this later cost-report source or the final delivery branch.

## Evidence and boundaries

[Exact native acceptance](desktop-performance-cpu-acceptance.json) and
[original raw archive](data/desktop-performance-cpu-709de340.zip) bind the complete source, binary,
commands, adapter, all frames, six project/image files and original logs.
Archive SHA-256: `edf2656c77c9b400f44230dd263a012349b9091296aeeaa3b8919e7ad7035313`. Its internal inventory
independently binds each file. No binaries, new assets or third-party media are
included. Schema-1 archives require the collector at their original source;
current schema 2 never synthesizes missing historical CPU measurements.

This is offline serialized headless GPU latency on an uncontrolled development
desktop. There is no native window/display deadline, real video file decoding,
physical memory exhaustion, controlled thermal/power experiment or before/after
speedup claim. GPU root/presentation timestamps still exclude some earlier
YUV/nested/vector/MSAA work; waited wall time includes submitted completion wait.

R60.1/2 package/program CPU buffers and R60.3 report integration gain actual new
coverage. Existing texture/admission observations keep their original scope.
Original-source input/image worker buffers, other containers, inference weights/
activations, native decoder/driver residency, physical Adreno/Mali and the other
original R60.4/A60.1/A60.2 requirements remain pending. Android is unmaintained;
#61's desktop-only scope does not close #60's original device checklist.
No panel, token, layout, interaction, dependency or third-party asset changed.

## Delivery identity

The implementation was applied to main after own #442 merged. Delivery code
`51095815f4cd364c0d52d405dd85ae902ff6ba1c` matches the measured source
for all 20 files introduced by this increment, before completion-record updates.
The ownership acceptance manifest records their Git blobs and hashes. Remaining
source differences are already merged desktop/UI paths; no private merge history
is published. Final-head CI is required before merge; these local measurements
remain attributed to source709de340, not a rebased or documentation commit.
