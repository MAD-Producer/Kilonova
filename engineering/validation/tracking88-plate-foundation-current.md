# External tracking: calibrated plate geometry foundation

Issue [#88](https://github.com/MAD-Producer/Kilonova/issues/88) remains open.
This increment implements independently authored CPU geometry in
`motion_host::tracking::plate`; it does not advertise a tracker, solver or
project-import capability. No UI, panel, dependency, asset or Android change.

Baseline is the locally available main commit `8573d997`. Refreshing main on
2026-11-01 failed because the configured loopback proxy was unavailable. This
branch must refresh/reconcile the actual remote baseline before integration.
Existing local and remote branch names were checked before starting; no
tracking-import branch was found. The root worktree belongs to other work and
was not edited. The #82 validation worktrees remain frozen and independent.

## Implemented contract

- Source/output extents are nonzero and bounded to 32768 pixels per dimension
- Camera coordinates are +X right, +Y down, +Z forward. Pixel coordinates start
  at the upper-left image edge; the first pixel center is `(0.5, 0.5)`
- Positive independent focal lengths and finite principal point describe a
  pinhole camera. Named Brown-Conrady distortion uses radial `(k1,k2,k3)` and
  tangential `(p1,p2)` coefficients in normalized camera coordinates
- Source projection and Euclidean residual use continuous source pixels
- Plate processing order is crop, clockwise quarter-turn rotation, then resize.
  The inverse preserves subpixels and points outside the crop/image; no clamping
- Invalid calibration, crop, non-finite input and numeric overflow return distinct
  causes. `BehindCamera` is a normal visibility disposition, not an import fault

World/camera-axis conversion, world units, fisheye/rational lens models and lens
undistortion are not implemented or silently approximated. An eventual importer
must declare these conventions and explicitly reject unsupported models.

## Verification

Eight pure CPU tests passed with the exact new `plate.rs` as the Rust test crate:

```text
rustc --edition 2021 --test crates/motion-host/src/tracking/plate.rs -o E:/Dev/aem/artifacts/pr-queue/tracking88-math-v1/plate-tests.exe
E:/Dev/aem/artifacts/pr-queue/tracking88-math-v1/plate-tests.exe --test-threads=1
```

Controls include independently calculated projections/residuals, separate radial
and tangential distortions, known values for all four rotations, 400 inverse
mapping points with subpixel/off-image coordinates, pixel-center conventions,
invalid metadata, behind-camera visibility and finite-range overflow. No existing
assertion was changed. Source was formatted and whitespace checked.

This isolated test avoids starting another Cargo/GPU validation while #82's full
workspace gate runs. It does **not** prove the host crate integration, project
roundtrip, actual camera rendering or complete issue acceptance. Full Cargo checks
and regression remain pending on the integrated source.

## Original issue scope still pending

Versioned point/plane/camera asset parsing and bounded resource limits; source
PTS/rational-fps metadata and precise time mapping; explicit pose/axis/unit
conversion; independent pose tracks and object/camera bindings; editable
corrections and saved visibility/confidence; persistence/relink/undo/cache
invalidation; synthetic-camera asset roundtrip and real calibrated plate footage;
actual rendered reprojection/lens-replacement goldens; a later planar tracker
prototype. A complete 3D solver remains a later staged feature, as the issue states.
Prerequisites [#85](https://github.com/MAD-Producer/Kilonova/issues/85) and
[#86](https://github.com/MAD-Producer/Kilonova/issues/86) remain open. This geometry
foundation neither changes the project's fps representation nor bypasses their
integration contracts. No full-feature or issue-completion claim is made.
