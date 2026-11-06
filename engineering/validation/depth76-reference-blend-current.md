# Issue 76: coordinate foundation for reference-depth mixing

Status: the inverse coordinate increment is implemented and tested. Depth
mixing, reference registration, scalar-expression reuse and temporal acceptance
remain pending. This record does not complete issue 76 or the 3D route.

The increment starts from maintained main
`26c1d2d2bc83c8073d85a200a7cc78cd6284d4f4`. Existing open PRs and related branches
were checked before starting; the new work is isolated from PR 606 and its
unchanged final-head CI. No panel, UI, dependency or asset changes are included.

## Implemented mapping

`DepthGeometry::depth_uv_to_source_uv` maps continuous oriented depth UV back
to full original-image UV. It undoes horizontal mirroring and clockwise
quarter-turns before applying the original crop. Texel centres use
`(index + 0.5) / size`; no integer rounding or edge clamp is introduced.

The existing validation runs first. Nonfinite inputs and invalid crop/rotation
retain explicit errors. Finite coordinates outside the depth plane return a
normal `None`, matching the existing forward mapping. This geometric inverse
does not register different cameras or presentation times and cannot establish
that two depth fields describe the same moving scene.

## Actual execution

The exact command was:

```text
cargo test -p motion-model --test depth --test depth_prepared --test depth_blend_coordinates --locked --offline -- --test-threads=1
```

All three complete selected targets passed: **12 passed, zero failed, zero
ignored, zero filtered**. The original controller exited zero after 171.703
seconds including fresh compilation; its recorded sources stayed unchanged.
Cache, target and temporary paths remained on E. Development/test debug data
and incremental compilation were disabled; no test assertion, threshold,
stack, runtime retry or existing dependency was changed.

Three new controls specify independently known source pixels for every turn
and mirror combination, check every crop corner and fractional centres, and
exercise out-of-plane/nonfinite inputs and invalid geometry. Existing depth
semantics and prepared-input controls also execute. A round trip alone is not
used as the orientation oracle.

The [archive](data/depth76-coordinate-inverse-v1.zip) and
[manifest](data/depth76-coordinate-inverse-v1.json) retain controller, original
output, source hashes and tested source snapshots. All eleven archive members
were read back and verified: 50,622 bytes, SHA-256
`0a72cc1a526e12bb6ccb642780005dbabe381068752bb2954178c9ea44a7f11e`.

## Remaining delivery

- Bind both depth fields and the mask to explicit source/revision/PTS and
  registration metadata; keep invalidity separate from RGBA alpha
- Convert relative/metric/inverse units and direction explicitly before mixing
- Sample differing crops, orientations and resolutions in a common domain,
  retaining float values and bounded staging/sampling cost records
- Reuse generic scalar Lerp/Mask compilation rather than adding a proprietary
  complex node; preserve exact zero/one endpoint semantics
- Define per-frame reference registration for moving subjects and verify a
  sequence against independent depth/weight/ghosting and cost controls

No GPU, UI, full-workspace or temporal-quality acceptance is claimed by this
coordinate increment. Android remains outside the maintained scope.
