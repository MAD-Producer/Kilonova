# Frozen imported-camera PNG and MP4 output

## Problem and implemented behavior

Ordinary PNG and MP4 export created a scene without installing the export
renderer model-source catalog before sampling. An imported camera therefore
could not resolve its owned model source. Both paths now install the catalog
prepared from the frozen source copies before sampling. The existing exposure
path already installs this catalog separately and is unchanged.

Two real output tests first failed on the missing imported-camera source, then
passed with this wiring. The initial red-v1 invocation stopped at lockfile
validation before compilation; it is retained as an invocation error, not a
product failure. Red-v2 contains the actual test failures. No existing test
assertion or threshold was changed to obtain a pass.

## Verified output

- Unorm8, Unorm16, Float16 and Float32 use the same independently authored
  imported mesh/camera fixture, with a non-square composition and a fixed-aspect
  source-camera viewport. Native camera positions are checked against the
  independent source-animation clock at every tested frame.
- Each MP4 freezes the original project, persists the snapshot, changes the
  live camera clock, closes both live and in-memory frozen owners, removes the
  original source directory, restores the durable snapshot and encodes it.
- All 31 frames of each 96-by-64, 30-fps movie are decoded with the actual FFmpeg
  backend. Tests verify frame count, complete duration, indexed PTS, color tags,
  visible animated geometry, viewport pillars and per-frame triangle coverage
  and centroid against the original native frames. Lossy H.264 comparisons use
  explicit coverage and pixel-position tolerances; byte equality is not claimed.
- PNG captures the original frame 15 before a live clock edit and source/session
  removal. Actual output pixels match the corresponding native capture exactly
  at all four precisions. This test also runs without FFmpeg and participates in
  normal desktop-media GPU testing.

The complete FFmpeg desktop-media library regression passes **54
tests**, zero failed and zero ignored, including the existing explicitly manual
keyframe-timing proof via `--include-ignored`. The separate no-FFmpeg PNG test,
no-default-feature media check, FFmpeg/AI-depth desktop compilation, provenance
and panel-style checks pass. Every step checks its captured source hashes.

This is affected-suite local acceptance, not a final full-workspace or
cross-platform #87 claim. Calibrated-video geometry, continuous-playback depth
reuse, complex shared-depth routes, DOF and final normative comparisons remain
open. Android is retired and excluded. There are no panel or style changes.
The sole added dependency edge is a test-only path dependency on the repository's
existing `motion-scene`; no external dependency version or asset was added.

## Evidence

The acceptance source snapshot was captured on parent `d18aadd77d2108b87801348f61cc4d9c9db3931c`;
the implementation commit before evidence packaging is `f3ade82c7efa332957ba114ac852814656943f21`. The archive
contains commands, original output, source hashes, changed source, the shared
fixture generator, four actual MP4s, eight comparison reports and original
native/decoded PNG samples. These are independently authored generated fixtures.

[`data/frozen-imported-camera-native-v1.zip`](data/frozen-imported-camera-native-v1.zip)

- SHA-256: `2f10f54088f89f4a9bbaf04a9a2b2607f16046fb0c595c8f9fcf7b3e49223136`
- Entries: 78; every entry was read back and compared with its original
