# Latest-main shared model/plane depth and calibrated output integration

## Final source and behavior

Main `a8b5ac1938d956f2aa3da25e7400fcb203234f24` is integrated into the owned PR528 branch. Every prior
owned patch Git blob is unchanged by that integration. The two completed
adjacent render increments are cherry-picked without changing their product or
test blobs. Final executed product source is `0c84d9ee79d9a4de5f7534673d1c118e222ab0e7`; the subsequent
publication commit adds only this document and evidence.

Compatible contiguous models and actual opaque solid 3D planes now share a
common camera and hardware depth. Ordinary 2D backgrounds, separators and
foregrounds retain authored compositing positions. Pure-plane scenes retain
their original BSP path. Immutable per-encode geometry, uniforms and admitted
lifetimes preserve pending/reversed/abandoned frames. Two fixed MSAA cache slots
include their native attachments in memory ledgers and retire on deletion.
Cold model receiver/exposure input preparation and explicit calibrated native
depth seeding are included in the same accepted source.

## Executed acceptance

The final current-main union has **532 passed, 0 failed, 0 ignored**: 93 model
data tests and 439 native rendering tests across 19 targets. Provenance,
panel-style checks and desktop FFmpeg/AI-depth compilation pass. Coverage
combines calibration, native scenes, models/animation, actual mixed planes,
receiver/exposure paths, 2D compositing/BSP, masks, effects, nested compositions
and multiple cameras. Prior overlapping checkpoint counts are not added.
Independent world rays, all four precisions at 1x/4x, saved/frozen assets,
source deletion, exact budget refusals/recovery, pending animation frames,
abandonment and retirement have actual native coverage.

The original controller stops after all 439 native tests pass because its
estimated total of 434 omitted five earlier all-model project tests. The
corrected finalizer parses and verifies the original 19 successful target
summaries, retains that original count-guard failure, then performs only the
remaining desktop check. No completed test is rerun and no assertion, product
source or executed input is changed to correct the arithmetic.

The unchanged external owning-worker harness is rebuilt against this exact
source. Its first build stops before compilation because newly merged main
adds a motion-core path-dependency edge to the standalone harness lock. The
offline workspace-only refresh changes only motion-core's dependency list,
adds/removes no packages and changes no registry package. Harness code and
assertions are unchanged; the original build failure and before/after locks
are retained. The subsequent locked/offline rebuild passes.

Fresh actual Session/native FFmpeg/CPU ONNX acceptance checks all **6,144**
color and hardware-depth pixels. Maximum absolute hardware-depth error is
`1.7886120418086904e-08` against the unchanged
`< 1e-6` bound. Decoded RGB and inference output are bit-exact to the previous
independent official-reference gate. Default Info starts are visible in the
file; a real stale-PTS Error appears in both file and stderr, while stdout is
empty. Supplied illustrative anchors do not prove metric model prediction.

The original PR528 head completed all five required CI checks successfully;
two release-only checks were intentionally skipped. A merge guard then refused
to merge because main had advanced. No remote merge occurred. This checkpoint
aligns main and batches already completed adjacent geometry work after that
CI finished. The updated PR must pass its own current-head cross-platform
checks; old green checks are not claimed for the new head. This is not a new
full-workspace or cross-platform execution claim.

## Boundaries and remaining work

This implements physical depth for opaque solid 3D planes, not textured or
transparent planes. Effects, masks, mattes, adjustments, nesting, non-normal
blends and incompatible sampling retain their receiver boundaries. Mesh alpha
uses global primitive-center ordering; exact intersecting transparency and OIT
remain outside this increment. Imported-camera selection and project-routed
camera tracks remain open. Explicit calibration is exercised in an external
worker harness, not persisted or automatically selected for current GUI
project frames; arbitrary image-plane reprojection and the depth/DOF authoring
closure remain open with #74. Neither #74 nor #87 is closed by this checkpoint.

No panels, UI styles, product dependencies or third-party assets are modified.
Android is retired and excluded. The already delivered movie is not rerendered.
Existing assertions, tolerances, timeouts and budgets are not relaxed. Earlier
checkpoints retain their exact source and failure history; this checkpoint is
the final combined maintained-main evidence for the current publication.

## Evidence

[Main alignment, exact source integration, complete current union, standalone
lock refresh and fresh actual outputs](data/depth-project-geometry-main-v1.zip):
SHA-256 `3b36b2d5facf43f1b0e152a4e6224408cf5a54c4b4be153a1a8227b1405942c5`, 1,953,188 bytes,
109 entries. Every entry is read back and hash-verified against
the manifest. Executables, shared libraries, model weights and bytecode are
excluded. Prior geometry failure histories remain in the linked checkpoint
archives under this directory.
