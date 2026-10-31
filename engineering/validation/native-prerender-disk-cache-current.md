# Optional native prerender disk cache

Issue: [#82](https://github.com/MAD-Producer/Kilonova/issues/82)

Status: integrated into draft PR 575 as
`8bb44d4a81f67f693a20e7bc3c067a987d37cb31`. Quota, storage, native components,
default desktop fault sinks and the actual Linux bounded-full-volume component
have passed on this source. Strict input P95, retained native crash causes and
final combined acceptance remain open. This document does not claim completion
of #82.

## Scope

- Disk caching defaults to disabled. MCP `configure_prerender.diskQuotaBytes`
  accepts zero or an integer from 81 bytes through 64 GiB. It uses the existing
  application data directory's `cache/prerender` subdirectory, never the project.
- Configuration does no filesystem IO. `diskConfigured` reports the selected
  policy; `diskCache` reports an actually opened backend. Headless configuration
  creates no worker or cache directory.
- The existing background worker opens, reads, hashes, trims and writes cache
  files. Its in-flight phases precede the actual disk calls. Cancellation remains
  cooperative; no timeout, replacement renderer or foreground filesystem wait
  is introduced.
- Entries retain native UNORM8, UNORM16, Float16 or Float32 bytes and the existing
  linear premultiplied contract. No SDR conversion or quantization is performed.
- Keys include content, output settings, frame, dimensions, contract, storage,
  cache format and a build-time producer fingerprint. Transient document and
  device generations are rebound to the current owner before installation.
- Producer identity hashes bounded regular repository inputs, compiler identity,
  target, features and build settings, with test/runtime separation. It is not
  an attestation of an external GPU driver or a dynamically replaced library.
- A fixed 80-byte header, expected payload length and checksum are validated
  before publication. Allocation uses the existing frame CPU admission and a
  bounded known layout. Scans retain at most 4096 entries. Payload IO checks
  cancellation in 64 KiB chunks.
- Writes stage in the cache directory and commit atomically. A nonblocking OS
  writer lock serializes quota accounting across cache owners. Contention skips
  a write. Eviction removes only recognized regenerable cache names; unowned
  files consume quota but are preserved.
- Optional cache IO faults retain their bounded original cause and path, fall
  back to native rendering, warn on the first consecutive occurrence, and emit
  Error once at occurrence 60. Re-preparing an unchanged unavailable directory
  does not reset open failures. Site success, normal misses, cancellation and
  changed configuration reset the relevant counter. Lifetime totals saturate.

## Regression coverage and actual disposition

1. Raw native bytes in all four precisions, including HDR/negative/alpha data;
   reopened entries and content/output/producer/layout invalidation
2. Truncated, corrupted, wrong-version, wrong-identity and oversized entries;
   regeneration and preservation of authored project data
3. Quota eviction, unowned-file preservation and concurrent real OS writer locks
4. Interrupted partial writes and cancellation with no published partial entry
5. Actual directory-open faults, first/escalated/suppressed observations and
   reset conditions at default Info
6. Actual worker cold/warm frames, generation rebinding, corruption fallback and
   no background GPU initialization on a disk hit
7. Controlled disk-open/read blocking with in-flight state visible before entry
   and CPU admission released after cancellation acknowledgment
8. Real foreground native installation/readback after the disk namespace has
   been moved away following background completion
9. Isolated MCP profile with no headless cache directory creation and no project
   history/revision changes
10. Actual desktop log-file output, Error stderr duplication and stdout isolation
    for 65 real failed directory-open attempts

The partial-write control injects an IO error after a real first payload chunk.
The actual Linux full-volume component has since passed; see
[its exact-source CI evidence](native-prerender-full-volume-current.md).
Windows/macOS full-volume behavior and physical storage durability are not
qualified. Strict foreground P95, the retained native abnormal exit, final
combined regression and complete current-head cross-platform CI remain open.
Android is retired and excluded as instructed.

## Executed results and failures

Three new quota controls first failed for their intended original assertions:
reopening with a smaller quota, reserving a new entry at the 4,096-entry scan
limit, and normal nonblocking admission while another actual writer owns the
cache lock. A production-only `disk.rs` repair made the identical three tests
pass. Their source hashes and original failures are retained. Admission now
trims before warm reads, reserves the prospective entry count, and reports
normal `openHolds` without confusing lock contention with an IO fault.

Storage controls passed 12/0/0; disk worker controls passed 4/0/1; the real
foreground installation control passed 1/0/0; the explicitly selected native
four-precision control passed 1/0/0. The latest media prerender group passed
63/0/3 after adding the explicitly ignored Linux full-volume test. These groups
overlap and are not additive. Their exact source boundaries remain in receipts.
Headless MCP policy passed 1/0/0 with a recorded exit code in the subsequent
portable-source gate; desktop check, tool inventory and provenance passed.

The default desktop fault-sink test failed in native WGSL parsing during
`Session` construction, before its disk-fault exercise. The actual abort reports
`ptr::offset_from_unsigned requires self >= origin` in the Naga lexer searcher.
The original EXE/PDB, hash-verified copies and raw stack are privately preserved.
A separate CPU-only default-thread parse of the three unchanged utility shaders
passed once; it neither establishes a cause nor fixes the original failure.
That failure remains unresolved. The failed test was not rerun unchanged to
seek green. On the subsequent integrated source `8bb44d4a`, the default-sink
control passed: 65 real failed directory opens produced two file fault records
(first Warn and Error at occurrence 60), one Error on stderr, and none on
stdout; the Info start record was visible. This changed-source result accepts
the sink behavior without explaining or erasing the old Naga abort.

Earlier preparation failures are also retained: crate-prefixed exact child
selectors selected zero tests; the one-test guard caught them. New native disk
fixtures lacked project directories. A JSON schema constant needed an explicit
u64 suffix with the same 64 GiB value. These fixes changed no existing assertion.
The quota controller later stopped without a terminal receipt after writing a
successful raw MCP summary. Its return code was not accepted from stdout alone;
a separate stopped-process observation preserves that limit. Completed quota
negative/fixed records remain valid independent evidence. The later changed
portable-source gate recorded the MCP exit code directly.

CI-tool verification initially lacked the existing pinned PyYAML 6.0.3 path.
After that environment correction, an unchanged workflow-policy assertion
caught a new evidence-upload step running on PRs. Its condition was corrected
to non-PR events, preserving the user's three-platform PR/no-package-upload
policy. Both affected workflow modules passed with their assertions intact.

Archive: [partial disk-component evidence](data/native-prerender-disk-components-v1.zip).
SHA-256: `b544ac8a0f0048d8f1039f56ef5672e53c56a7e37c9e554a2e3c24ba80e0d43f`. All 129 members were read back byte for byte.
The archive preserves actual commands, source hashes, original failures and
subsequent scoped results. Private executables/PDB and dumps are excluded.
The final documentation/archive were added after source-bound execution;
no Rust implementation or test bytes changed while those gates ran.
