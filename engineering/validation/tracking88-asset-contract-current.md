# External tracking typed asset contract

Issue [#88](https://github.com/MAD-Producer/Kilonova/issues/88) remains open.
This increment adds `motion_host::tracking::asset`, a typed CPU validation
boundary alongside the [calibrated plate foundation](tracking88-plate-foundation-current.md).
It is not yet a JSON/file importer or a project editing operation.
The later [JSON decoder increment](tracking88-json-decoder-current.md) adds the
bounded read boundary; this document retains the original typed-only evidence.

## Validated semantics

- Version must equal the current contract version, without migration/defaulting
- Source identity includes a bounded identifier, content hash and source extent.
  Actual content-hash verification belongs to the pending source connector
- Signed, strictly increasing source PTS preserve negative/nonzero origins and
  VFR gaps. Exact source seconds remain an integer tick/timebase fraction
- Bounded, reduced nominal source-rate metadata does not regenerate timestamps
  or change the project's current fps representation
- Point, plane and camera observations cannot be coerced into each other
- Planar homographies map a declared reference source frame's pixel coordinates
  to current source pixels. Reference indices must identify that same source;
  homography scale is preserved. Maximum-entry-normalized determinant magnitude
  must exceed `1e-12`; singular or numerically poorly conditioned matrices are
  rejected with a distinct cause rather than silently repaired
- Camera-to-world poses retain position and XYZW unit quaternions. Explicit
  meters-per-world-unit is required for camera tracks. Per-observation camera
  calibration permits animated focal/principal/distortion metadata and must
  match the original source resolution
- Sparse samples reference actual source PTS by index. Visible samples require
  observations; occluded/lost samples may retain predictions or have no geometry.
  Confidence stays finite in `[0,1]`. Validation does not infer interpolation,
  reset confidence, erase manual corrections or promote predictions to visible
- IDs are unique and bounded. Structured error causes retain track/sample indices
  and avoid copying user identifiers into error messages
- Limits: 4096 tracks, one million source PTS, one million total observations,
  128 bytes per identifier and 256 KiB aggregate identifier storage

Validation is read-only. It neither sorts samples nor normalizes supplied pose or
homography data. These limits check an already constructed typed asset; they do
**not** bound a decoder's earlier allocations. The pending byte decoder needs its
own input-byte, sequence-length, string-length and nesting limits before allocation.
An external convention/units/lens adapter must explicitly transform unsupported
metadata; no missing-field defaults or implicit axis conversion are specified.

## Actual verification

The exact new asset/plate source files were included by a tiny standalone Rust
test harness. `rustc --test` plus the resulting executable completed successfully:
**20 passed, zero failed, zero ignored** (12 asset controls and eight original
plate controls). No original assertion changed. The initial harness used an
incorrect `#[path]` module-directory assumption and failed before executing tests;
the harness was corrected to include the two source modules explicitly. This does
not establish host-crate module integration, which still requires Cargo.

New controls cover exact VFR/negative-origin fractions, nominal-rate reduction,
version/timebase refusal, duplicate/reversed PTS, sparse indices and error context,
occlusion/manual data preservation, non-finite confidence/coordinates, bounded
and duplicate IDs, empty/excessive track input, camera units/quaternions/source
calibration, scale-invariant homography conditioning, reference identity and
cross-kind refusal. Existing plate tests remain in this same executable.

The harness and raw log are retained under
`E:/Dev/aem/artifacts/pr-queue/tracking88-asset-v1`. No new Cargo/GPU/native-window
validator was started while #82's frozen full workspace gate runs. No dependency,
third-party asset, panel, UI or Android change was introduced.

## Remaining integration

Bounded versioned byte decoding/serialization, source identity verification,
explicit world/axis conversion, camera/object/parent or independent pose-track
binding, interpolation policy, manual editing, undo/redo/save/reopen/relink/cache
invalidation, exact project-time mapping, actual rendered reprojection and
calibrated real-footage/lens-replacement goldens remain pending. Typed validation
alone does not complete an import requirement. #85/#86 prerequisites, updated-main
reconciliation, full host/Cargo regression and exact-head CI remain pending.
No tracker or full 3D camera-solver capability is advertised.
