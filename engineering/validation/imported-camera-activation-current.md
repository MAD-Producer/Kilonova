# Imported camera project activation

## Implemented boundary

Camera instances store an explicit managed source, scene and camera-node selection,
unit conversion, pivot and independent imported animation clock. Active camera
layer precedence and composition-local frame offsets remain unchanged. Existing
parent links place the camera instance in a rig; spatial children inherit its
resolved rigid world matrix. Observer views override the view without changing
these authored hierarchy results. No camera is selected automatically.

The command `camera_imported` selects an instance or explicitly clears it with
`binding: null`. Omission is rejected. Core validation checks managed references;
host preparation checks the actual frozen scene/node/clip before committing the
edit. Invalid candidates retain the project, revision, history and owned catalog.
Clipboard dependency closure and asset remapping include camera layers and nested
composition cameras. The absence of an imported selection is semantically optional,
not a legacy-format migration.

One immutable bounded CPU catalog feeds semantic hierarchy sampling and GPU
uploads, including no-GPU sessions, nested and exposure scenes, project replacement,
history, composition switching and actual current-frame PNG capture. An unchanged
frame performs no new source-file reads. Cross-root replacement validates the new
root; it cannot borrow an identically named source from the old root. IO failures
include source, phase and managed path and retain their native IO cause.

Imported projection retains near/far planes, perspective/orthographic type and
fixed aspect. Native mesh passes use the original projection and camera viewport;
ordinary 3D planes use its equivalent composition projection and homogeneous
viewport clipping. 2D layers and hits retain the full composition. Source cameras
require the resolver at sampling time; an unsampled empty Scene is not a rendered
fallback camera.

## Validation status

Source-bound local acceptance has **646 passed, 0 failed, 0 ignored**:
93 motion-model tests across 23 targets (including doc tests), 80 affected core
tests across 15 targets, 448 native renderer tests across 18 targets, and 25 host
tests across six targets. Desktop FFmpeg/AI-depth compilation, provenance and
panel-style checks also pass. Eleven new tests comprise three core, six native
and two host tests. These are selected affected suites, not full-workspace,
cross-platform or final issue acceptance. Earlier checkpoint counts overlap and
must not be summed as additional coverage.

The gate ran against main `7dfab923a1b4005e2c8b31460bbe766738f78a10`
and source head `1df5b06dc6eb4998431cbcc03b934286295fb0d7`, verifying all
tracked source hashes after every step. The later commit amendment changes only
this document and its evidence archive. Existing assertions, thresholds, skips,
timeouts and budgets are unchanged. Initial fixture/oracle corrections are
described below; the archive contains the complete frozen successful run.

Tests cover storage reopening, source-file removal, independent camera animation,
rewind pixels, parent rigs and children, camera layer clocks, observer precedence,
nested FPS conversion, exposure receiver clocks, exact Float32 color with a
separately rendered shutter alpha average, actual host PNG bytes, host selection
undo/redo, invalid selection rollback and clipboard dependency remapping. The
Float32 shutter oracle avoids per-receiver UNORM8 quantization; original existing
UNORM8 tests and thresholds remain unchanged. Test geometry/buffers are authored
in repository code, with no third-party or Adobe assets. Only existing workspace
motion-scene and already locked sha2 are added to host test dependencies.

## Remaining boundaries

This is command/API activation, not a new UI camera picker. No panel, style or
interaction code changes. Source cameras retain their imported quaternion tracks;
authored camera track controls do not retarget those tracks. Core-only operations
which require a resolved source camera matrix (for example preserve-world
reparenting or scene-prefix drag conversion) still refuse without a resolver.
Preserve-local parenting and source-independent edits retain their existing paths.

The frozen media/video package route has a separate Scene constructor and remains
unverified; another active branch owns that area. Current-frame PNG acceptance
must not be described as complete video export acceptance. Effect/mask/matte and
nested shared-depth boundaries, intersecting transparency, persisted current-frame
depth calibration and reprojection/DOF with #74, and final maintained-desktop
cross-platform acceptance remain open. Android is retired and excluded. Neither
#74 nor #87 is closed by this increment.

[Source-bound acceptance](data/imported-camera-activation-native-v1.zip), SHA-256
`c633e9901eb76cbc2d7ea2267554793352aaae6bf796fb5ef5be9177d450cabf`, 1044114 bytes. The archive retains exact
changed source, the patch against the tested main baseline, commands, raw logs
and exit receipts. Every entry is read back and SHA-256 verified.
