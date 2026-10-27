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
