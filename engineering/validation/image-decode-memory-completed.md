# Image decode CPU ownership observation — completed increment

Issue #60 remains open. Measured implementation: `73bb7a47d92c6b67e98a7949cf6b4fd55bafd3e7`. Parent delivery
source: `cc1622dac90812eea450a873dec0fe90311e9f0f`. This increment measures actual owned CPU vectors; it does not
claim complete memory accounting, physical residency or playback performance.

## Behavior delivered

- Publish unknown before allocation, then actual vector length/capacity for the
  RGBA output, PNG/JPEG downsample accumulators and returned JPEG image backing
  storage. Typed element sizes are included. Codec internal allocations remain
  unknown rather than inferred from dimensions or requested budgets.
- Keep storage and its observation lease together through decode, bounded result
  queue, cancellation, native error, panic and disconnected receiver. Storage
  drops before its lease. Cancellation alone is not evidence of memory release.
- Add `cpu.imageDecodeTasks` to the explicit Session resource query, summing
  original-input, preview-renderer and editor-renderer task owners separately.
  Actual transfer to retained source caches is counted once. Equal source IDs
  do not alias independent allocations.
- Copy bounded three-slot metadata with `try_lock` before JSON construction.
  Busy/poisoned state is unavailable/unknown. Queries do not poll the channel,
  read source files, initialize graphics, change pixels/history or retain data.
  No UI, dependencies, assets, retry/restart or cancellation semantics changed.

## Actual acceptance

The complete, unfiltered command was:

```text
cargo test --locked --offline -p motion-render -p motion-host -j1 -- --test-threads=1
```

Across its actual test binaries: **659 passed, 0 failed,
1 existing ignored**. Ignored tests are listed in the acceptance JSON;
they are not accepted coverage. Provenance and `git diff --check` also passed.
The test target reused dependency artifacts with Cargo fingerprints and disabled
incremental compilation; it was not a fresh build. All artifacts/temp paths were
on E:. The source remained committed, clean and unchanged throughout the run.

All 14 new tests actually executed. They use real PNG/JPEG/Adam7/proxy decoding,
controlled allocation/queued-result barriers, spare capacity, cancellation,
panic, native errors, disconnected receivers, contention and poison recovery.
Host tests exercise the public resource query with the file renamed, preserve
project/history/owners, transfer bytes between task and cache, retain queued
cancelled storage, and create independent editor/original tasks. The editor test
uploads/renders the decoded image and checks the actual red center pixel.
These are correctness checks, not an interactive FPS or throughput study.

## Retained negative evidence

The first full command on `d763ee065490f969125e24c11235ad2049695ba5` failed its new spare-capacity fixture:
an untyped vector inferred `i32`, so actual capacity was 388 bytes rather than
the fixture's expected 97 bytes. The successor adds only `Vec::<u8>` to that
fixture. No assertion, test selection or production behavior was changed to
obtain acceptance. Its original log/receipts and the exact one-line fix are in
the same archive as the complete successful successor rerun.

## Reproduction evidence and remaining scope

`image-decode-memory-acceptance.json` records exact test names, every binary's
counts, source blobs, command, archive hash and the original failure. The archive
also includes the full implementation delta from its published base. Archive
`data/image-decode-memory-73bb7a47.zip` has 16 evidence entries plus a SHA-256 inventory;
SHA-256: `9c4ccf3970b135d55c486f66ded63ad200447ad1ac30892bb025e76e73cd82e6`.

Codec internals, retired workers, source-reader/proxy metadata and escaped
pixels remain explicit unknown/excluded owners. Snapshots are sequential and
not frame peaks. Earlier source709de340 performance records are not relabeled
to cover this implementation. Full #60 still needs real file-video/window
studies, complete GPU-stage timing, pressure/external/inference coverage,
physical Adreno/Mali and controlled optimization before/after evidence.

## Delivery source identity

Latest-main base: `00627cdb98fb874595f8ef05c2fa5e4d426741cc` (includes the actual Chinese #448 merge).
Implementation delivery: `d684b638a84d29752b5d31083b5ac01b478d8a00`. All ten implementation/test/API file blobs
are identical to measured source `73bb7a47d92c6b67e98a7949cf6b4fd55bafd3e7`. Only the tested increment
is applied; pending/private PR ancestry is not published. The original complete
local test counts remain attributed to their measured source. Integration
checks belong to the final PR head's required CI, not a relabeled local run.
