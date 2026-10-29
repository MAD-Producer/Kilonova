# Indexed video depth: scalar spill storage

This increment continues issue #87 after PR #546. It implements session-scoped
file-backed scalar retention and verified worker recovery. It does not close
the issue or claim that file-backed frozen exports are complete.

## Implemented behavior

- An authored indexed video track now retains completed scalar samples in one
  anonymous temporary file. No RGB frames, inference runtime or GPU handles
  enter the spill file. The live scalar/RGBA cache keeps its existing allowance
  and eviction policy.
- File creation, positional writes, checksum reads and recovery execute on the
  depth worker. The Session owner only constructs the store and reads its
  bounded metadata. The metadata mutex is separate from the writer mutex and
  is released before any IO or logging.
- Re-requesting an archived interval stages the actual original video again.
  Asset/object, source revision, geometry/model, actual indexed PTS/end and
  prepared RGB fingerprint must match before its scalar result is reused.
  Corruption or changed source pixels fail explicitly; they do not silently
  run inference or substitute another interval.
- Every restored scalar plane is checked against SHA-256, bounded dimensions,
  validity/confidence semantics and the existing depth-resource contract.
  Little-endian float bits, including masked NaN payloads, are preserved.
  Independent positional cursors permit concurrent reads without shared seeks.
- Store capacity is 4,096 samples across the session and 2 GiB of physical append
  extent. Superseded physical records still consume that extent; this increment
  does not claim compaction or unlimited duration. A single restored resource
  retains the existing 192 MiB allowance. Capacity failures preserve earlier
  records and report the request, object and allowance.
- Canceled/failed writes truncate only their newly appended tail and never
  publish a partial record. If rollback also fails, the original failure remains
  the primary cause.
- Document generation, engine revision, composition, project root and decoded
  image allowance scope ownership. A changed scope or explicit depth cancellation
  disposes of the store. Existing outside-clip cancellation semantics remain.
- Session status exposes stored sample/byte counts and saturating capacity
  rejections. Those storage observations reset when the store is disposed;
  worker restored/completed counters retain their worker-lifetime scope.
- Non-Present worker start and archive read/write phase records precede IO at
  the default Info level. In-flight phase state remains published until the
  result is completed. IO/integrity/capacity failures use the existing Error
  infrastructure; no product output is added to stdout.

## Verification

Source-bound gate: `artifacts/pr-queue/video-depth-store-v1`.
The source-bound gate passed: **227 distinct tests passed**, zero failed or
unexecuted. Its one inherited manual media test was explicitly executed and passed
on the same source. Host without AI depth and desktop with FFmpeg/AI depth
checks, style and provenance checks passed. This is related regression evidence,
not final full-workspace or cross-platform issue acceptance.

[Archived evidence](data/video-depth-store-native-v1.zip), SHA-256:
`da30f626d99ee4d7c7d86cdb939ee223b602ca3984580133bc4eae565b848c8d`

All 162 entries were read back and verified by SHA-256.

The new tests cover chunk boundaries, exact float/mask/confidence restoration,
file ownership after store disposal, concurrent readers, independent byte and
sample capacities, changed input guards, corruption/truncation, cancellation
before work and between actual IO chunks, rollback and recovery, and preservation
of the primary IO failure when rollback also fails.

The controlled indexed-decoder integration test authors 150 intervals, admits
all 150 at the final document revision, observes 128 live versus 150 stored
samples, and recovers the evicted first interval without another backend call.
An independent test checks that source decoding still occurs on recovery and
that document replacement/cancellation resets store ownership. A real tempfile
creation failure after controlled decoding must produce a default-level start
and contextual Error and must not publish a successful result.

The 150-sample fixture is a controlled six-second source. It proves retention
beyond the live FIFO; it is not sustained real-codec playback or inference
throughput evidence. Existing real-codec/frozen export tests remain part of the
regression gate, with their previously documented scope.

## Remaining work

- Capture archived file planes into a metadata-only frozen snapshot after live
  eviction; the current frozen snapshot still requires all authored samples in
  the live cache and retains its 128-entry/192 MiB bounds
- Stream/restore bounded temporal file planes in persistent independent exports
  without materializing an entire movie's depth planes in RAM
- Exercise sustained real-codec playback and export beyond the live cache
- Complete complex effects/masks/mattes/nested/exposure depth, intersecting
  transparency, DOF and final current-main/cross-platform issue acceptance

Android is outside the maintained acceptance scope. No panel, layout, style,
asset/dependency or compatibility migration is introduced by this increment.

The subsequent [file-backed frozen export increment](video-depth-file-export-current.md)
implements archived capture and streaming restoration, with a real-codec
150-frame ownership proof. Its separate acceptance and remaining limits apply.
