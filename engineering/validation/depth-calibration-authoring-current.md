# Persisted depth authoring and current-result admission

## Implemented boundary

The worker fingerprints its actual RGB8 sRGB opaque inference input after source
decode, crop, orientation and video transfer conversion, before entering the
backend. SHA-256 covers a versioned domain, little-endian dimensions and prepared
pixels, with cancellation checks in bounded chunks. Preparation timing includes
hashing. The worker alone creates the immutable Estimate/input pair: a backend
cannot supply the authoritative input digest. Public direct Estimate/Backend
contracts remain intact. Reading an installed session fingerprint does not
perform file IO, hashing or inference.

SavedDepthCalibration stores validated bounded authoring data, not inferred
pixels or a live edit revision: exact source asset and rational PTS, actual input
digest and extent, output interpretation/geometry/model processing, two anchors,
curve/extrapolation and explicit positive scene pixels-per-metre. The model's
processing revision is retained. Rebinding requires matching current prepared
input and interpretation, then creates a new immutable runtime calibration using
the actual admitted resource revision. The original RelativeDepthCalibration
revision checks remain unchanged.

An optional per-layer calibration is meaningful absence, not a migration.
`depth_calibration` requires an explicit nullable calibration field, supports
clearing, locks, scoped transactions, ordinary undo/redo, save/reopen and clipboard
asset remapping. Image/video references and geometry are validated against the
project. Host authoring requires the actual installed current result, validates
the transaction's final candidate before commit and preserves rollback/history
and the original error. A command committing a calibration changes the edit
revision, invalidating the previous installed result: only an explicit fresh
request can establish a subsequent runtime binding. Loading, undo and copying
saved authoring data do not implicitly run inference.

## Verified scope

The source-bound local gate passes **317 tests, 0 failed, 0 ignored**: model 96
across 25 targets, selected core 68 across 13 targets, and host 153 across nine
targets including actual native GPU library tests. Host compilation with depth
disabled and desktop FFmpeg/AI-depth compilation also pass, as do provenance and
panel-style checks. These are affected suites, not full-workspace, cross-platform
or final issue acceptance. Existing assertions, thresholds, budgets, timeouts and
ignored-test annotations are unchanged. Twelve new tests are included in these
totals; repeated attempts and checkpoint counts must not be summed.

Tests independently verify fixed hash vectors, changed pixel/dimension identity,
BT.709-to-sRGB conversion before hashing, geometry and bounded digest validation,
live revision refusal, saved interpretation/PTS/model refusal, mandatory nullable
command schema, locked/invalid atomic rollback, clipboard ownership mapping,
actual managed PNG preparation, committing/undoing/redoing a calibration,
save/reopen into another live revision, pending/canceled/stale results and
explicit recovery. Changing only an image's green channel while the controlled
backend reads red leaves predicted values unchanged but still refuses the saved
calibration, proving predicted values or path metadata are insufficient identity.
External file changes are detected on fresh source preparation; this does not
introduce per-frame file polling or claim synchronous detection of disk mutation.

The gate uses source base `8b1610282864e4738c7fe245f1ab9e9644c5f0f4`,
whose tree was merged as main `724ea8c0151b92b95c125ddd60a4b8c655230ecc`.
Each step verifies the exact source hashes. The later completion commit adds
only the document/archive beyond those tested implementation bytes.

The first run passed all 317 tests, then its depth-disabled compile encountered
stale trait metadata from the previously built camera-editing worktree. The
cached trait required a method absent from the unchanged current source; Cargo
dep paths in the shared target were relative. Current workspace input mtimes
were refreshed without changing file bytes, product behavior or assertions, and
the entire gate was rerun. Both raw attempts and the diagnosis are preserved.
Earlier prepared-input gates did not execute tests: the first stopped because
its prerequisite camera gate failed, and the second was stopped to expand this
same batch into persisted authoring. Those receipts are not passed coverage.

## Remaining route

This stage does not feed depth to ordinary host preview/capture or shared model
passes. SceneDepthInput remains a standalone admitted renderer input; native
frame routing, a source-camera/plane reprojection contract, multiple calibrated
layers, common-unit validation across consumers, camera motion/aspect, immutable
GPU seeding with budget refusal/recovery and actual reprojection/DOF output remain
open. Camera ancestry editing is separately accepted in PR535 and is not part of
this source base. Frozen video export, complex effect/mask/matte/nested shared
depth, intersecting transparency and final desktop acceptance also remain open.
Neither #74 nor #87 closes. Android is retired and excluded.

No UI/panel/style changes, third-party assets or new dependency versions are
introduced. Host production hashing opts into the already locked sha2 0.10 under
the existing optional ai-depth feature; the default session still starts no
inference worker. No source-camera or model-scale assumptions are silently added.

[Source-bound evidence](data/depth-calibration-authoring-v2.zip), SHA-256
`fc83c50020728c7f62c99de0cf98fe32e29072b8f61a16c1f4d6940ec6e64ce8`, 552064 bytes. Exact changed source,
commands, controllers, raw output, exit receipts and initial stopped/failed gates
are retained; every archive entry was read back and SHA-256 verified.
