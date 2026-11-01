# External tracking JSON decoding boundary

Issue [#88](https://github.com/MAD-Producer/Kilonova/issues/88) remains open.
This increment adds `motion_host::tracking::decode::{from_slice, from_reader}`
on top of the [typed asset contract](tracking88-asset-contract-current.md).
It returns a validated independent asset. It does not modify a project, source,
camera, layer or animation track, and is not an advertised tracking feature.

## Wire contract

The JSON object uses these required fields:

| Field | Meaning |
| --- | --- |
| `version` | Exactly the current asset contract version |
| `source_id`, `source_sha256` | Bounded source label and 64 hexadecimal content-hash digits |
| `source_extent` | `[width, height]` in original source pixels |
| `coordinates` | Explicit `right_down_forward` camera/world convention |
| `ticks_per_second`, `nominal_rate`, `source_pts` | Integer timebase, nominal rational rate and exact signed source PTS |
| `tracks` | Point, plane or camera tracks with unique IDs and sparse samples |

Each track requires `id`, `kind` and `samples`. Plane tracks additionally require
`reference_index`. Each sample requires `source_index`, `visibility`, `confidence`
and `manual_correction`. `observation` can be absent/null only when visibility
allows missing geometry. Observations are explicitly tagged with `kind`:
`point` has `pixels`, `plane` has a 3x3 `matrix`, and `camera` has
`camera_to_world` plus `calibration`. Camera poses require `position` and
`orientation_xyzw`. Calibration requires `source_extent`, `focal`, `principal`,
`radial` (three Brown-Conrady coefficients) and `tangential` (two coefficients).
Camera assets require positive `meters_per_world_unit`; it is otherwise optional.

No unknown fields, duplicate keys, extra JSON documents, unsupported enum values,
implicit axes, missing required fields or silent repairs are accepted. Direct
deserialization avoids losing duplicate keys in an intermediate JSON value map.
Visible geometry and all existing typed semantic checks run before returning.
Manual intent, off-image points, negative PTS origins, VFR gaps, sparse samples,
homography scale, per-sample calibration and world-unit metadata stay intact.
The source hash is decoded but not yet compared to actual footage bytes.

## Resource and error boundaries

- Input is limited to 16 MiB. The reader consumes at most limit plus one byte
  before rejecting an overlong stream; it does not read to an unbounded EOF
- Sequence visitors impose the typed per-sequence limits while decoding, without
  reserving space from an untrusted size hint. IDs are bounded in UTF-8 bytes
  before being copied into owned strings
- Fixed scalar-vector fields explicitly consume their array boundary and report
  a structural shape error when an extra component appears
- Aggregate observation/identifier limits are also validated after wire decoding.
  This is not a byte-exact global memory reservation: parser scratch storage,
  internally tagged enum buffering and simultaneous wire/typed values have
  overhead beyond the input byte count. The reader is bounded in bytes, not time;
  a reader that never returns still needs the eventual worker's in-flight state
- JSON errors retain a fixed structural reason plus line/column, excluding raw
  keys, enum strings and full input text. Geometry/semantic errors retain bounded
  track/sample locations and original typed causes. Underlying I/O causes remain
  accessible through `Error::source`

This pure library boundary performs no logging or I/O beyond its supplied reader.
The future import command must add the existing default-level start/error records
and source/request context before invoking it; that integration is pending.

## Actual verification

A standalone CPU harness includes the exact product `plate.rs`, `asset.rs` and
`decode.rs`, using the repository's existing serde/serde_json build dependencies.
It ran **31 tests: 31 passed, zero failed, zero ignored**: 11 new decoder controls
plus the original 20 geometry/asset controls. No original assertion changed.

The new tests exercise real JSON bytes/readers: point/plane/camera conversion,
exact timing and manual state, unknown fields at six object boundaries, required
metadata, duplicate keys including tagged observations, trailing documents,
unsupported enums, malformed shapes/nesting, UTF-8 and sequence limits, bounded
read consumption, original I/O causes, invalid hashes/calibration, semantic error
locations and exclusion of long credential-like text from parser errors.

The first compile exposed tests referring to private calibration fields; read-only
parameter accessors were added and the tests use those APIs. The first executable
run retained **30 passed, one failed**: serde_json's fixed-array decoder classified
an extra source-extent component as syntax. A bounded explicit vector decoder
corrected that product error classification; the failing assertion was retained.
Both the initial compiler log and failed test log are preserved alongside the
final log and hash receipt in `E:/Dev/aem/artifacts/pr-queue/tracking88-codec-v1`.

No additional Cargo/GPU/native-window validator was launched while #82's frozen
full workspace gate runs. This direct compilation does not establish host-crate
integration or replace full Cargo/CI acceptance. No dependencies, external assets,
panels, UI or Android files changed. The branch is based on cached main `8573d997`;
remote refresh, current-main reconciliation and Cargo acceptance remain required.

## Remaining #88 work

Serialization/round-trip persistence; actual source-content verification; explicit
external convention and project-time conversion; camera/object/parent or pose-track
binding; interpolation/manual editing; undo/redo/save/reopen/relink/invalidation;
actual rendered reprojection and calibrated-footage/lens-replacement goldens.
Prerequisite #85/#86 integration remains open. This is neither a tracker nor a 3D
camera solver, and it does not complete the full issue.
