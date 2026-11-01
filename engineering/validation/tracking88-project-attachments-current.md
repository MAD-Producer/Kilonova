# Managed original tracking attachments

Issue #88 remains open. This backend increment adds byte-exact original tracking
documents to project resources, history, composition views and portable project
archives. Final validation ended at `2026-11-01T04:36:56.837754+00:00` on parent
`167e734eb3d502527cb2a0f491f7e581e6273432` plus the archived source delta. The fetched main baseline was
`8573d9975cf70d2c8fe05605102ec9d744816070`. It adds no panel, project-browser item, menu or evaluated pose
binding. Current project schema is **32** with a required `tracking_assets` list;
older schemas are intentionally rejected without migration or missing-field defaults.

## Resource and history contract

`TrackingAsset` records a globally unique numeric resource ID, an explicit image
or video source reference, a managed `assets/tracking/*.mstrack` path, a versioned
format tag, declared bytes, and separate original-document/original-source SHA-256
identities. Documents are limited to 16 MiB and 256 attachments per project. Normal
path containment, unique resource IDs and existing total package budgets apply.

`tracking::project::Attachment::prepare` strictly decodes the bounded original,
checks a fresh complete-source verification proof, compares original image/video
dimensions and sample-count constraints, and produces one core registration
command. The source reference is selected explicitly by the caller; imported text
cannot select a source path. Original JSON whitespace and number spelling are
retained rather than replaced with normalized encoder output. Source PTS, manual
corrections, confidence and lost/occluded metadata remain unmodified.

Core registration participates in atomic edits, undo/redo, resource-change
classification, the headless command catalogue and exact serialized history
accounting. Composition views retain the project-wide attachments. Undo changes
references while retaining the owned original file for redo. Subsequent image,
audio, video, model, text-raster, library-image, clipboard and preset allocation
skips registered tracking IDs; integer exhaustion remains an atomic error.

Save/load and archive export/import now include and verify these managed originals.
Fixed-buffer hashing checks exact file length and content; bounded reads recheck
identity after acquisition. Frozen package preparation rechecks the copied bytes,
including same-length changes after snapshot creation. Missing files retain typed
IO phase/path/original cause; content errors identify the managed path. Failed save
or package preparation preserves the prior project/output. `load_original` verifies
both the stored original document and a fresh caller-provided source stream.

## Actual controls

| Source snapshot / control | Exit | Passed / failed / ignored |
| --- | --- | --- |
| V1 tracking76 | 0 | 76 / 0 / 0 |
| V1 model-core-tests | 0 | 639 / 0 / 1 |
| V1 desktop-check | 0 | compile/check only |
| V1 host189 | 0 | 189 / 0 / 0 |
| V2 tracking78 | 0 | 78 / 0 / 0 |
| V2 clipboard-preset-tests | 0 | 23 / 0 / 0 |
| V2 media-unit-tests | 0 | 99 / 0 / 0 |
| V2 desktop-check | 0 | compile/check only |
| V2 depth-feature-check | 0 | compile/check only |
| V2 host191 | 0 | 191 / 0 / 0 |
| V2 whitespace | 0 | compile/check only |
| Shared-ID negative, before correction | 101 | 0 / 1 / 0 (expected) |

V1 ran complete model/core regression before the shared-ID correction. V2 reran
the affected clipboard/preset integration tests, all media and host unit tests,
and desktop/default plus ai-depth compilation. V1's full model/core total must not
be presented as an exact final-source full-workspace result. The original ignored
test remains ignored; no pre-existing assertion, sample count or acceptance
threshold was weakened. One owned Cargo/native validator ran at a time.

The 12 new tracking tests use independently generated PNG bytes and actual core
history/save/reopen/archive paths. They cover raw formatting, original metadata,
same-length document/source replacement, post-snapshot mutation, missing-file IO
causes, cancellation, invalid source bindings/IDs/extents, descriptor/document
identity mismatch, required schema fields and resource budgets, composition/history
retention and exact undo/redo serialized-byte accounting. Video admission checks
use an explicitly opaque byte stand-in: original/display extent, timestamp-count
and encoded-byte-count checks are unit coverage, **not a video decoder/PTS proof**.

The added cross-project image clipboard control first failed at the intended
`asset IDs must be unique and nonzero` error with one failed test. After correction
the unchanged control passes, including `u64::MAX` exhaustion without mutation.
The negative test file is a byte-exact prefix of the final file; the only appended
control covers video metadata. That identity receipt is retained with raw logs.

Archive [source snapshots and raw controls](data/tracking88-project-attachments-v2.zip),
SHA-256 `9c2de6691b16b1169c0ab48a0d7c9b27db5dfc2c92604def4fd5add2d7c78a31`, 90 read-back-verified members.
It includes initial, negative and final source snapshots, guards, deltas and raw
logs; no executable, PDB, private event data or third-party asset is published.
No dependency, lockfile, UI/panel or Android change was introduced.

## Remaining acceptance boundaries

This is a core/backend attachment increment. A real external import/file-publication
transaction and desktop/MCP routing are still required. It does not add pose
evaluation, camera/object/parent bindings, live relink integration, decoded original
PTS/dimension agreement, proxy correspondence or runtime cache invalidation.
Metadata comparison is not independent decoder evidence. A source proof describes
bytes actually read and is not a filesystem lease. Cooperative cancellation cannot
interrupt an already blocked reader. Connectors must publish request/operation/
in-flight context before IO and report typed causes outside business locks.

Independent pose tracks, explicit gap/manual policies, lens replacement, #85/#86
project integration and rendered synthetic/calibrated real-footage controls remain
required. Full current-source workspace, actual desktop/MCP and cross-platform
qualification are not established here. Android remains excluded at the user's
request. No solver or complete tracking feature is advertised.

The preceding [original-source verifier gate](tracking88-original-source-current.md)
failed with 178 passes and a native child `0xc0000005` access violation. Its exact
binary/PDB/private events remain retained. Later V1/V2 runs use changed source;
their successes do not explain or repair that native fault and do not replace the
failed receipt. The original #82 strict P95 failures also remain failed; no unchanged
performance gate was rerun by this increment.
