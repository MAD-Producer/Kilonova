# Imported camera ancestry editing

## Implemented boundary

Parent-prefix and single-world-matrix queries evaluate only the required ancestry.
An unrelated source camera cannot block an ordinary layer edit. A required camera
still needs an explicit resolver. Managed source metadata is checked against the
owned catalog before sampling; changed source identity cannot silently reuse an
older camera matrix. This validation is metadata-only, with no per-frame file IO.

The host borrows its immutable catalog for one editing transaction, including
composition-scoped commands, preserve-world and preserve-local parenting,
frame-aware removal and clipboard cuts that detach children. The catalog is not
stored in model history or a global/thread-local variable. Failed source sampling
or external preparation uses the existing rollback path and preserves revision,
history and the original failure. Drag and existing 2D align/distribute operations
convert displacement through the resolved ancestry. Their existing supported
layer types and interaction rules are unchanged; this does not add 3D alignment.

## Validation status

The frozen local regression gate passes **748 tests, 0 failed, 0 ignored**:
93 model tests across 23 targets, 80 affected core tests across 15 targets,
450 native renderer tests across 18 targets, and 125 host tests across seven
targets. Desktop FFmpeg/AI-depth compilation, provenance and panel-style checks
also pass. Four new tests (two renderer/semantic and two actual host operation
tests) are included in those totals. This is selected affected regression, not
full-workspace, cross-platform or final issue acceptance. Prior checkpoint
counts overlap and are not additional coverage.

The gate ran against main `7dfab923a1b4005e2c8b31460bbe766738f78a10`,
dependency head `8b1610282864e4738c7fe245f1ab9e9644c5f0f4` and source head
`e92635a68037b924de725e662311ef4ecc1684af`. Every source hash was verified
after each step. The completion amendment changes only this document and the
evidence archive. No existing assertions, tolerances, timeouts, budgets or skips
were changed.

The first full gate caught an existing missing-source regression: early catalog
validation had dropped the original missing-source cause and required node
context. Product code now preserves both. The unchanged existing regression test
passes in v2. Its failed v1 output and all initial focused fixture corrections
are retained alongside the successful gate.

Coverage includes a rotated rig and independent source clock, source-file removal,
unrelated parenting, explicit resolver refusal, preserve-world transforms, scoped
commands, both parenting policies, camera-layer cut/removal, exact undo/redo,
external-preparation rollback, changed source metadata refusal, actual host drag,
2D alignment/distribution bounds and failure rollback. Reopening uses restored
original managed bytes: storage intentionally refuses saving a project with a
missing managed source even when a live session still owns the parsed catalog.

Initial focused attempts corrected new test-fixture assumptions: Camera content
owns a Box; an identity detached parent is canonically None; a rotated selection's
leftmost layer is not determined by object order; and persistence validates owned
files before writing. These corrections do not change existing assertions,
tolerances, timeouts, budgets or ignored tests. Raw focused attempts are retained
for the later acceptance archive.

## Remaining scope

This is API/host editing, without a camera picker, panel, layout or style change.
Source quaternion tracks are not retargeted to authored Euler tracks. A batch
which changes the required source metadata must prepare the matching catalog
before a source-dependent parent edit; stale metadata is explicitly refused.
Frozen video export, effect/mask/matte and nested shared-depth boundaries,
intersecting transparency, persisted current-frame calibration, reprojection/DOF
and final maintained-desktop acceptance remain open. Neither #74 nor #87 closes.
Android is retired and excluded. No dependencies or third-party assets change.

[Source-bound evidence](data/imported-camera-editing-native-v2.zip), SHA-256
`5e50a269c51055fb4cce3be20b90387ab57cb3b34e8c009e101e938ce929e9f2`, 435239 bytes. It retains changed source,
the dependency-relative patch, exact commands, raw logs, exit receipts and prior
failed attempts. Every archive entry was read back and SHA-256 verified.
