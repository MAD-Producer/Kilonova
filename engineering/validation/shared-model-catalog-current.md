# Shared CPU model source catalog

## Implemented

`ModelSourceCatalog::prepare` freezes a validated replacement without a GPU.
The immutable catalog map and parsed resource payload use Arc ownership. Clones
and the native renderer share those same parsed scenes. Preparation retains the
existing 320 MiB encoded and 320 MiB retained-payload limits; cache hits are
charged too. These limits apply to each admitted catalog, not process RAM or
concurrent old/new generations. Existing budget assertions remain unchanged.

The renderer can adopt a prepared catalog before asset configuration, and return
a shared read-only catalog to semantic consumers. Asset synchronization still
validates source identity and authored references. No frame-time IO or parsing
is added. Candidate failure does not change the previous catalog. Duplicate
source IDs now fail explicitly rather than overwriting a map entry; this is a
functional validation fix, separate from logging. GPU/source/target invalidation
otherwise keeps the existing installation behavior.

## Acceptance

Source-bound native acceptance: **425 passed, 0 failed,
0 ignored**, 13 targets, plus desktop FFmpeg/AI-depth compilation, provenance
and panel-style checks. Existing assertions are unchanged. Four new tests cover
CPU-only save/reopen, source deletion, cross-thread Arc identity, retained byte
accounting, failed replacement, duplicate ID refusal, empty replacement with
live readers, and actual new-device upload/capture after files are deleted.
The native capture is byte-equal to an independent earlier device capture and
remains unchanged after failed reconfiguration.

No UI/style/panel, dependency or third-party asset changes. Android is retired
and excluded. This is not full-workspace or cross-platform acceptance.

## Remaining camera integration

This checkpoint provides resource ownership, not imported-camera activation.
Saved camera selection, hierarchy and independent animation clocks still need
to feed semantic scene sampling before effects and nested/exposure sampling.
The host's initial no-GPU sampling, replacement, undo and capture/export must
use the shared catalog. Current-frame depth calibration/reprojection/DOF and
intersecting transparency also remain open. Neither #74 nor #87 is closed.

[Source-bound native evidence](data/shared-model-catalog-native-v1.zip), SHA-256
`5a16c30ddcf046a2241909ca71c158432942fac16977b33e51da8b0bf772e51d`, 237217 bytes. Every archived entry is read back
and hash-verified. Counts overlap previous checkpoints and must not be summed.
