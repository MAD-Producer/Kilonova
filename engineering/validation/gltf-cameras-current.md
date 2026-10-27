# Imported camera checkpoint

## Implemented scope

Issue #87 now imports typed core glTF perspective and orthographic camera
descriptors and node references. A resource containing cameras is no longer
rejected wholesale. Descriptors are counted against item and retained-payload
limits; invalid projection types, references, clipping intervals and shapes
remain explicit errors. OBJ sources keep an empty camera collection.

`Scene::camera_view` explicitly selects a camera node reachable in a scene,
samples its transform clip and hierarchy, converts metre positions and clipping
distances into pixels, and ignores scale for its camera basis. Finite/infinite
perspective and orthographic projection produce native [0,1] depth. Authored
aspect ratios produce a centered viewport instead of cropping or stretching.
This follows the public [Khronos glTF camera specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#cameras);
no third-party implementation or assets are copied.

`SceneFrame::from_imported_camera` and `ScenePassFrame::from_imported_camera`
consume that view in actual single-source and shared native rendering. Viewport
validation precedes native draw recording. Existing callers default to the full
target. There is no automatic activation, authored composition-camera mutation,
camera-selection UI or project setting in this checkpoint.

## Executed acceptance

The exact executable snapshot based on `430fe572` passes 107 tests with zero
failures and zero ignores: 67 scene import/animation/camera contracts, five model
project contracts, 34 actual native GPU/model tests and one real editor keyframe
test. Desktop FFmpeg/AI-depth check and provenance also pass. Every step verifies
all Rust/Python/Cargo source hashes remain unchanged during execution.

Seven new CPU tests independently cover native clipping endpoints and projection
scales, infinite depth, authored aspect and letterboxing, hierarchy and explicit
animation sampling, owned frozen replay, invalid inputs and budget refusals.
The scene traversal shares a visitor rather than allocating all node instances
for each existing mesh query. Existing mesh and animation tests execute again.

Two new GPU tests render from imported camera nodes, compare single-source and
shared-pass pixels exactly, read real Depth32Float against analytical clipping
references, inspect actual transparent letterbox rows and unchanged depth there,
and verify invalid viewport failures preserve prior pixels. The prior ten
shared-depth GPU tests also execute unchanged, including budget rollback and
four-precision/HDR acceptance. This does not rerender the delivered model movie.

The existing unsupported-feature aggregate test stops asserting that cameras
must be rejected because camera support is implemented. Its skin assertion is
retained; new malformed-camera rejection tests replace that former capability
boundary. Existing assertion logic, timeouts, numerical bounds and ignore
annotations are not weakened to obtain acceptance.

Evidence: [exact sources, hashes, patch and raw execution outputs](data/gltf-cameras-native-v1.zip),
SHA-256 `4bee9db31d13f6c19e06839c319b3259b5435ec79d1627c7106addad60eb46d8`;
1,181,005 bytes, 49 entries, each read back and verified. This is targeted local
acceptance, not full workspace or cross-platform CI for this newer source.
PR #522 contains the preceding model/depth milestone, not these later cameras,
imported transform animation or shared native depth changes.

## Remaining limits

Selected camera nodes with collapsed, reflected or sheared global bases are
explicitly refused; no arbitrary orientation is manufactured. This is a current
view-sampling profile limit, not a claim that all such assets violate glTF. Their
ordinary geometry can still import. Extreme values that overflow native
projection or lose direction precision also fail explicitly.

Project camera selection, mixed mesh/plane and nested/matte/effect composition,
skin/morph animation, calibrated AI-depth conversion and final maintained-main
regression/CI remain open. This checkpoint does not complete #87. Android is
retired and excluded. No panel/UI file, external asset or dependency changes.

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
