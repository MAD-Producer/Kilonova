# Tracking #88: certified lens inverse and GPU output primitive

This checkpoint follows `a044a16f481da7a6174ef44fdb18525c70ac17e6` on the tracking import branch.
It implements necessary output primitives. It does **not** connect nonzero lens
distortion to a composition camera and does not close #88, #82 or the 3D route.
The calibrated camera consumer still explicitly refuses nonzero coefficients.
Android is retired. No panel, UI, interaction, dependency or third-party asset
was changed. Fixtures and code were independently authored.

## Implemented items and boundaries

- **LENS-1 / new model primitive:** Brown-Conrady radial k1/k2/k3 and tangential
  p1/p2 retain independent fx/fy, principal point, focal correction and plate
  crop/rotation. The documented forward model is referenced from the
  [official OpenCV calibration documentation](https://docs.opencv.org/4.10.0/d9/d0c/group__calib3d.html).
  No upstream implementation, dependency or asset was copied. The new inverse
  uses a fixed 48-step contraction, not the separate Newton test oracle.
- **LENS-2 / admission:** over the radius R=2D+epsilon disk, displacement Jacobian
  norm B is bounded by 3|k1|R²+5|k2|R⁴+7|k3|R⁶+8(|p1|+|p2|)R. B must be below .45.
  This proves uniqueness and convergence on the entire output domain. Lens
  domains outside this conservative proof are explicitly refused, including
  some physically valid lenses; an arbitrary sampled grid is not a proof of
  non-folding. Float coefficients, focal/principal/domain ranges and residuals
  are checked. This admission is not a universal subpixel f32 error guarantee.
- **LENS-3 / overscan:** inverse Lipschitz bound 1/(1-B) encloses every point in
  each normalized output cell, then transforms complete conservative bounds
  through calibration and plate coordinates. Raster allocation adds two
  physical guard texels, has explicit scale/side/16-megapixel limits and gives
  the producer a matching projection window. Identity retains the original
  rectangle. Out-of-domain queries and insufficient footprints return errors.
- **LENS-4 / new GPU primitive:** reusable device/format-owned pipeline consumes
  an already composed linear premultiplied spatial raster. Manual bilinear
  texture loads avoid optional Float32 filtering/blending. Four native channel
  precisions retain their storage; there is no tone map, clamp, alpha conversion
  or display LUT. Hardware sRGB transfer for UNORM8 is honored. Identity at the
  same physical extent copies exact texels, including odd dimensions.
- **LENS-5 / resource and failure controls:** source/destination/encoder/pipeline
  device ownership, aliasing, storage, dimensions, color meaning and certified
  footprint are checked. Each encode has an immutable 128-byte admitted uniform,
  retained by the actual allocation encoder through submission or abandonment.
  Budget refusals preserve typed resource errors and context. The primitive
  returns causes to its caller and does not add healthy file IO or polling logs.
  Default-level worker request/start/error observations are existing coverage;
  a new native camera-distortion logging path is **not yet exercised**.

## Actual validation

The source-bound serial controller and complete commands/logs are archived.
Full model: **242 passed / 0 failed / 0 ignored** including doc
tests. Selected renderer targets: **135 passed / 0 failed /
0 ignored**, with GPU tests enabled. These include the four new lens controls,
image capture, resource retirement, shared planes/meshes/depth/alpha, imported
camera, scene GPU, explicit/shared views and multiple compositions. FFmpeg
desktop check and provenance validation passed. These are selected controls,
not a full desktop/workspace or cross-platform qualification.

The independent analytic Newton/Jacobian oracle checks every RGBA output byte
for four rotations and both signed radial cases with nonzero tangential terms,
crop and asymmetric intrinsics. All eight actual GPU captures have maximum
byte error <=1; skipping distortion is detected at 535–598 pixels per case.
The comparison decodes sRGB source bytes, interpolates linear light and encodes
the destination transfer. Actual PNGs and per-case results are archived.

Other GPU controls use spatially varying identity fields at all four precisions,
negative/HDR Float16/32 fields, nonlinear constant fields with alpha, two
different lenses queued before a single submission, exact 127/128-byte budget
boundary, retirement after completion and target-owner release before submission.
CPU controls cover zoom, all rotations, independent Newton reprojection,
continuous-footprint admission, preview scales, projection window/depth and
explicit numeric/folding/capacity refusals. No existing assertion was weakened
or gate disabled. All new GPU tests ran; none was skipped.

## Failed attempts retained

Initial compiler errors were a comparison of an owned texture wrapper with a
native texture, a shadowed calibration variable in the test oracle and the
integration-test submodule path. They were corrected directly. The first GPU
oracle run was refused because its pipeline requested plain UNORM8 while the
capture contract negotiated sRGB storage. The test now selects actual storage
and its independent oracle accounts for the documented transfer. The original
one-byte tolerance was preserved. Earlier logs and successful intermediate
logs remain in the archive.

The previous calibrated-camera renderer compilation access violation remains
unexplained. This checkpoint changes renderer source and therefore legitimately
compiles a new artifact; a successful new build is **not proof that rustc was
repaired**. Earlier native/Naga/MCP compiler faults and #82 strict P95 failures
and incomplete GPU coverage remain unresolved. No unchanged failed gate was
rerun merely to obtain green.

## Remaining work

Connect this primitive after depth/alpha composition of each contiguous spatial
run, preserving authored 2D boundaries, independently selected/nested cameras,
shared-frame/frozen views and exposure samples. Carry matching forward/inverse
mapping into projected anchors, picking and view-plane drags. Unsupported
spatial effects/matte/depth combinations must refuse explicitly until implemented.
Only after that routing and native acceptance should the current nonzero lens
refusal be removed. A genuinely calibrated permitted-source real clip and
independent reprojection/lens-replacement evidence remain necessary; synthetic
GPU checker controls are not proof of real-footage tracking. Vendor adapters,
relink/cache controls and full maintained regression/CI remain open.

Archive `tracking88-lens-primitive-v1.zip`: 162546 bytes,
31 members; SHA-256 `c542d3c711983ea4075c672956072c302bd93460c525504e3dad04424d3a2860`.
Every archived member was read back and byte-compared. The closeout JSON records
source hashes, counts and remaining scope. Origin main was fetched during this
validation; HEAD already contained main `8573d997`, with no incoming commits.
