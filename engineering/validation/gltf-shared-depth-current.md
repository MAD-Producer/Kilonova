# Native shared scene depth checkpoint

## Scope

Issue #87 now has an explicit native pass that combines several owned model
sources and repeated instances before rasterization. It clears color and depth
once, uses one camera and explicit MSAA 1 or 4, and globally orders opaque/masked
draws before far-to-near blended primitives. Per-instance transforms, lighting
and imported animation samples remain independent. Original single-source APIs
use the same draw preparation, validation and immutable uniform machinery.

This is a native rendering foundation. Project model layers still rasterize in
isolation; this checkpoint does not claim shared mesh/plane composition, camera
selection UI, calibrated AI depth, skeletal animation or completed #87.

## Resource and failure contracts

Depth/MSAA textures and immutable uniforms use the existing shared renderer
admission budget. The pass bounds both instance count and combined primitive
count, including repeated source references. It retains exact renderer and
owner-thread identity. Allocated command reservations survive dropping sources,
pass objects and output handles, and retire on submission completion or encoder
abandonment. Attachment replacement commits only after all preparation and
allocation succeed; a partial MSAA allocation failure preserves prior attachments
and output pixels. Source pipeline cache admission can still occur on a rejected
frame; output mutation is deferred until all draw preparation succeeds.

`ScenePass::depth()` exposes admitted hardware depth in the selected camera's
projected [0,1] space. Single-sample depth allows COPY_SRC for real readback.
Multisample depth has no implicit resolve. It is not a relative or metric
`DepthResource`; conversion and calibration remain explicit subsequent work.

Failed pass calls retain original instance/request/source/phase context. Their
counter is scoped to one pass, saturates at 61, reports Error on occurrences 1
and 60 and resets on successful encode/draw. Healthy calls add no file IO or
routine Info records; existing pipeline creation retains its Info start signal.
The counter measures calls, not elapsed seconds. This checkpoint reuses the
previously verified application sink; it does not replace its file/stderr tests.

## Executed acceptance

The exact executable snapshot based on `317371f1` passes 98 tests with zero
failures and zero ignores: 60 scene import/animation contracts, five project
model contracts, 32 actual native GPU/model tests and one real editor keyframe
test. Provenance and the desktop FFmpeg/AI-depth check also pass. Every step
verifies all Rust/Python/Cargo source hashes remain unchanged. No existing test
assertion, numerical tolerance, timeout or ignore annotation is weakened.

Ten new GPU tests independently verify:

- Separate source meshes occlude by actual hardware depth regardless of input order
- Blended primitives from two sources interleave globally over an opaque source
- Discarded masks and blended fragments leave opaque depth intact
- Repeated source instances and two pending encodes preserve distinct uniforms
- A one-byte MSAA color admission refusal rolls back partial attachments and recovers
- Foreign thread/renderer, absent source, singular frame and explicit sample refusals
- Abandoned commands release all retained source and attachment capacity
- Four working precisions, actual HDR irradiance and empty-pass color/depth clear
- Common perspective camera projection, world translation and metres-to-pixels scale
- Combined primitive limits, Error suppression at 1/60 and recovery reset

Color and Depth32Float values are read from the actual GPU. Perspective depth
uses an independent homogeneous projection reference. Transparency uses an
analytical premultiplied-over reference. Native PBR, texture, source budget,
imported animation and editor keyframe regression tests also execute unchanged.

Evidence: [exact source hashes/files, outputs and native editor fixture](data/gltf-shared-depth-native-v1.zip),
SHA-256 `52ac8d974552ef3e00308c42db5d6b3f49cda9339df6f5cad06a76985f4700de`;
1,152,166 bytes, 41 entries, each read back and verified. This is local targeted
acceptance, not full workspace or cross-platform CI for this newer source.
PR #522 contains the preceding model/depth integration, not this later change.

## Remaining boundaries

Project mixed mesh/plane ordering, nested compositions, masks, mattes, effects,
layer opacity and source reads must be integrated without changing the existing
plane BSP contract. Intersecting transparent triangles still require a separate
OIT policy; global primitive sorting is not a solution for them. Imported
skin/morph/camera support and calibrated AI-depth conversion remain open.
Android is retired and excluded by user instruction. No panel/UI file, external
asset or dependency changes, and no repeat render of the delivered model movie.

## Maintained-main full regression

The later animation, shared-depth and camera changes are now replayed over the
maintained main source `bcaeb8d0`. Frozen executable source `b2005f5e` passes
3,291 tests across 319 result targets, with zero failures and
18 existing manual ignores. All 18 literal
manual annotations are inventoried before execution; each observed ignore maps
uniquely to an identical whole-file main Git blob. Platform-uncompiled manual
tests are listed separately in the receipt. No tests, assertions, tolerances,
timeouts or ignore annotations are weakened.

The run excludes retired Android and enables desktop FFmpeg, AI-depth and the
native GPU features for render, host, UI, desktop media and graph. Style-consumer
and provenance guards, desktop check/build and the actual binary command listing
also pass. The listing is valid JSON containing `register_model_asset`, with
empty stderr. Every stage verifies unchanged source and a clean frozen tree.

Parent PR #522 is merged as `650d0a01`, with its Chinese squash title verified
from the actual GitHub commit. The publication branch replays only the three
later feature commits over that parent. Every tracked non-documentation input
matches the frozen full-regression tree; the three original differences are
parent completion documentation/archive files. This proves source equivalence,
not bit-identical binaries across different build paths. Final documentation is
added after testing and does not modify Rust, Cargo or runtime inputs.

Main advanced to `06fa8cf5` while this frozen gate was running, introducing
layer playback switches in model, snapshot and renderer inputs. Those later
changes are not covered by this local source identity. The PR merge-ref CI
must independently validate their combination with this stack; the archive
records this boundary without attributing old-tree success to untested code.

Evidence: [exact inputs, patch, raw outputs, manual inventory and publication
equivalence](data/gltf-maintained-main-b2005f5e.zip), SHA-256
`cfbf544dd6d2fa379ab5e1dc20a6234a5252a2051cf41b88f3095832ebff90c5`; 1,745,005 bytes, 107 entries,
each read back and verified. The archive excludes weights and native runtime
binaries. Cross-platform CI for this later PR remains a separate gate.

This supersedes the earlier local-only/full-regression-pending checkpoint, not
the remaining product scope: project mixed mesh/plane/nested/matte/effect
composition, skin/morph, calibrated AI depth and camera-selection UI remain open
under #87. Imported transform animation and native camera selection/projection
are implemented as described in the linked checkpoint ledgers. The delivered
model movie is not rerendered; no panels or UI behavior change.
