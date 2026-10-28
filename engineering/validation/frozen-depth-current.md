# Frozen calibrated-image depth acceptance

Issue: #87. Accepted implementation: `82af19eea61245ab81c128080e77691079bd6390`.
Main integration baseline: `72f1037aea7a71047f1920347756d63dedf3e41b`.

This increment retains independently admitted image-depth results in export
plans and verifies the actual copied RGB before attaching calibrated geometry.
It does not run inference on the export worker or weaken live frame/revision
installation guards. Capturing a pending result still fails explicitly.

## Actual output evidence

- Four precisions: Unorm8, Unorm16, Float16 and Float32
- Four PNG exports after seeking, closing the session and removing original sources
- Exact PNG/native RGBA equality plus independent near-red/far-blue occlusion checks
- Four real MP4 files restored from disk after both session and in-memory snapshot owners exit
- All 31 decoded frames of every MP4 checked for near/far occlusion and native pixel error
- Bounded lossy mean absolute error: below eight per RGBA channel
- Existing explicit SDR_CLIP conversion is used; four working precisions do not change output bit depth
- Corrupted persisted scalar payload rejected by integrity verification
- Imported-camera frozen PNG/MP4 regressions rerun in the same accepted source tree

## Ownership and persistence

The snapshot clones immutable scalar ownership without copying live planes.
The wire format stores scalar values, optional validity/confidence planes,
original provider, input fingerprint and calibrated source/layer mapping. No
inference backend, weights or GPU handles enter the queued snapshot.

At most 128 layer/resource entries and 192 MiB of unique scalar/validity/confidence
planes are admitted per snapshot. Metadata is limited to 8 MiB. These are per
snapshot limits, not a claim that all queued tasks share a 192 MiB process limit.
Restoration checks counts, dimensions and aggregate bytes before plane allocation.
Aliased layers retain one shared resource; masked NaN bits and confidence values
survive round trips. Cancellation is checked during chunked IO. Original-image
decode is bounded and cancellation is checked before and after decode.

The queued movie manifest now accepts version 2 only; old queued snapshots are
not migrated. Depth is hashed from a private copy before parsing, then rebound
only after the copied image and frozen project mapping pass verification.
Unsupported calibrated-depth exposure and nested/video routing fail explicitly.

## Validation

- Host library and integration targets: 217 passed, zero failed/ignored
- Full FFmpeg media library including existing manual timing test: 56 passed, zero failed/ignored
- No-FFmpeg calibrated PNG: one passed (overlaps the FFmpeg suite)
- AI-disabled scalar framing test: one passed (configuration coverage, not an extra unique test)
- AI-disabled media build and FFmpeg/AI desktop build passed
- Provenance and panel style checks passed
- All tracked source hashes checked unchanged after every command

The controlled backend is independently authored test data. These tests establish
ownership, geometry and export behavior, not pretrained model accuracy.
No product UI, panel layout, styles, external assets or external dependency
versions change. A test-only dependency on the existing motion-model crate was added.

Archive: [frozen-depth-native-v1.zip](data/frozen-depth-native-v1.zip).
SHA-256: `b3a4937a8ebe25494a9e2e01f70d4ac6d031ec9524617db4cbb4db9de8b19150`.
The archive contains the controller, commands, original stdout/stderr, source
hashes, changed source files, comparison reports and actual PNG/MP4 outputs.

## Remaining #87 scope

Static-image live continuous-playback reuse, calibrated-video geometry, complex
effects/masks/mattes/nesting/exposure depth, intersecting transparency, DOF,
normative glTF comparisons, final current-main full-workspace regression and
maintained-desktop cross-platform CI remain open. Android is retired and excluded.
