# Tracking pose conversion and world reprojection

Issue [#88](https://github.com/MAD-Producer/Kilonova/issues/88) remains open.
This increment adds explicit external world/camera coordinate and unit conversion
alongside the [JSON round-trip boundary](tracking88-json-roundtrip-current.md).
It is a host math API, not project camera activation, a tracker or a camera solver.

## Conversion contract

`tracking::pose::Basis` represents an independently selected signed permutation
of the three source axes. All 48 axis/reflection conventions are representable;
repeated axes, zero and out-of-range components are rejected. Named bases include
right/down/forward, right/up/back and right/up/forward. Names do not cause any
filename/importer-based inference.

`Conversion::new` requires independent world and camera-local bases, explicit
source/target meters-per-world-unit, and an origin in target world units. Column
vectors follow these equations:

```text
p_target = scale * B_world * p_source + origin
R_target = B_world * R_source * inverse(B_camera)
scale = source_meters_per_unit / target_meters_per_unit
```

World and camera bases must have the same determinant; otherwise a source rigid
quaternion would become a reflection, which cannot be represented by a rigid
quaternion. Such input is refused explicitly. Reflected conventions work when
both declared bases have consistent handedness. A source world's up axis is not
assumed to equal its camera-local up axis.

`Conversion::pose` validates input, converts translation and orientation, and
returns a separate pose. It interprets the existing validated near-unit quaternion
as a rigid rotation and emits a unit quaternion. This normalization is explicit
conversion behavior; the original imported pose/asset is never rewritten. It does
not infer camera movement, generate ordinary camera keyframes or alter confidence,
visibility/manual state. The existing pose validation was factored into
`asset::Pose::validate`; original finite/unit checks and their error locations are
preserved by the unchanged asset tests.

`Conversion::inverse` handles axes, units and origin together. Non-finite inputs,
invalid units, overflow/underflow that prevents finite positive scaling, and an
unrepresentable inverse return distinct errors. Coordinates are never clamped.

`camera_point` transforms a target-world point into canonical camera space.
`world_residual` combines that operation with the existing calibrated distorted
projection and reports a residual in original source pixels. World points and
camera translation must use the same declared unit. Calibration is for the
canonical camera/image convention; this API does not infer a lens model or
silently reinterpret an external intrinsic matrix. Crop/rotation/resize remain
the independent plate mapping; measurements are converted back into source pixels
before residual comparison. Behind-camera points retain their distinct visibility
disposition instead of being treated as successful projections.

These APIs have no logger or file/project side effects. Typed pose/projection
causes remain available through `Error::source`. The eventual import command still
needs request/source context, in-flight state and default-level error records.

## Actual verification

The standalone CPU harness ran the new pose tests with geometry/asset controls:
**29 passed, zero failed, zero ignored**. A combined direct source harness then
ran all existing JSON, asset and geometry tests with the new pose controls:
**44 passed, zero failed, zero ignored**. Original assertions were not changed.

Nine new controls verify:

- All 48 signed axis conventions and their inverse, plus malformed axes
- Independently calculated unit/origin/quarter-turn results
- Separate world/camera up axes and explicit handedness refusal
- All 1152 valid world/camera basis pairs, preserving known camera rays and
  inverse pose/point behavior under nonzero origin and nontrivial rotation
- A distorted reprojection golden calculated from the Brown-Conrady equation,
  including an independently known five-pixel residual
- External camera conversion followed by source projection and actual
  crop/clockwise-rotation/resize/inverse plate mapping
- Invalid units/points/origins, numeric overflow, unrepresentable inverse,
  invalid pose causes and behind-camera visibility

The first compilation caught incorrect test calls to the existing plate API
(`forward`/integer rotation); the fixture was corrected to call `apply` and the
typed rotation enum before any test ran. The compiler log is retained. Evidence
and source hashes are under `E:/Dev/aem/artifacts/pr-queue/tracking88-pose-v1`.
Existing repository glam/serde artifacts were used; no dependencies or third-party
assets were added. This direct compilation does not establish host Cargo, project
integration or rendered footage acceptance.

The separate owned branch stacks on `a58b252e`, leaving that exact source frozen
for its queued host Cargo check. Main was freshly fetched using a command-local
proxy override and independently verified through GitHub API as `8573d997`.
No persistent proxy configuration was changed. No second owned Cargo/GPU validator,
panels/UI, Android, deletion or other agent's source changes were introduced.

## Remaining issue scope

Canonical JSON plus conversion helpers do not constitute a complete external
format adapter. Persisted original-solve provenance and conversion metadata,
source identity verification, exact source/project time mapping, independent pose
tracks and camera/object/parent bindings, manual edits/history/relink/cache
invalidation remain required. Actual rendered synthetic-camera round-trip,
calibrated real footage and lens-replacement goldens remain pending. Prerequisite
#85/#86 integration and exact-head Cargo/full regression/CI remain pending; full
3D camera solving remains a later stage as specified by the issue.
