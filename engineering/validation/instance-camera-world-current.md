# Imported camera instance-world sampling

## Implemented

`camera_view_in_world` samples the explicitly selected source node and original
animation in pixels, then composes the same affine world prefix that the model
instance uses. The prefix includes the caller's model-layer hierarchy and pivot;
it does not bake camera animation into Euler tracks. `CameraView.world` exposes
the resulting orthonormal camera transform for semantic hierarchy integration.
The earlier `camera_view` API delegates with identity, preserving source units,
projection, clipping and authored aspect/viewport. Per glTF, camera scale is
ignored; collapsed, sheared, reflected, nonfinite or projective camera frames
are explicitly refused. Shared native depth attachments also expose TEXTURE_BINDING for original depth
samples at 1x/4x, after explicit usage capability validation, without a depth
resolve or additional texture allocation. Existing byte admission is unchanged.
No camera is automatically selected and no healthy
frame-time logs or file reads are introduced.

## Source-bound acceptance

The complete motion-scene suite has **85 passed, 0 failed,
0 ignored** across 9 targets including doc tests. Affected native
renderer acceptance has **106 passed, 0 failed, 0 ignored**
across five targets: model layers, scene GPU, multiple-camera compositions,
spatial effects and exposure frames. Desktop FFmpeg/AI-depth compilation and
provenance verification also pass. These are distinct scope counts, not a
full-workspace or cross-platform result. Earlier checkpoint totals overlap and
must not be summed as new coverage.

The first native attempt has 105 passed and one failed new test: its helper
incorrectly tries to copy multisampled depth through the single-sample-only
reader. That raw failure is retained. The helper now reads all four actual depth
samples through a compute shader; every sample remains compared without a
relaxed threshold or omitted MSAA case. No existing test helper/assertion changes.

Three new CPU tests cover perspective/orthographic coordinate covariance,
unchanged clipping and aspect, independent animation time, units and pivot
ordering, invalid-prefix refusal and recovery with the same owned source.
The new native GPU test moves an actual model and camera together through a
90-degree basis rotation and translation. Color bytes match the independently
rendered original in four working precisions at 1x/4x MSAA, and actual depth is
compared at the existing depth-test tolerance of 1e-6. Original assertions,
timeouts and budgets are unchanged; no old test is skipped or relaxed.

## Remaining integration

This is the imported-camera world transform boundary, not saved project camera
activation. The shared CPU catalog, saved selection, semantic hierarchy and
camera viewport must still be wired through initial/no-GPU host sampling,
replacement/undo, composition-local nested/exposure clocks, preview, and actual
capture/export. Fixed imported aspect must preserve clipping/letterboxing for
all scene consumers rather than only the mesh pass. Persisted current-frame
depth calibration, reprojection/DOF and intersecting transparency also remain.
Neither #74 nor #87 is closed. No UI/panels/styles, dependencies or third-party
assets change; Android is retired and excluded.

[Source-bound acceptance](data/instance-camera-world-native-v2.zip), SHA-256
`ab7a67d369b382c3e7af1b26b053f24c766d9eb7c48abd5bf05a55888556f908`, 317249 bytes. Entries are read back and
hash-verified, with the exact source snapshot and native logs retained.
