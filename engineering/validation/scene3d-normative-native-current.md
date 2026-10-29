# Independent native normative rendering checkpoint

Issue 87 requires normative material, skin and coordinate comparisons, not
only successful glTF parsing. This checkpoint renders unmodified pinned
BoxVertexColors and SimpleSkin with independent raw-source references.
It does not claim that the remaining official alpha comparison is finished.

## Provenance

Both samples come from Khronos glTF Sample Assets revision
`8f492390ddc7a2ae221a1369ef560589cf8c70fb`. BoxVertexColors and SimpleSkin are
by Marco Hutter, CC0-1.0. Exact URLs, sizes and SHA-256 values are in the
external manifests and receipts. `ATTRIBUTION.md` records the source and scope.
Original models, buffers, textures and upstream screenshots are not bundled
in this archive or in product assets. The archive contains validation
readbacks, reports and the independently authored probes/oracles.

## Default material, vertex color and coordinates

The Box sample is imported with its original implicit material, without an
authored replacement. Four native working precisions render one unit axial
light and no ambient contribution. Python independently decodes original
accessors, indices and vertex colors, computes world-coordinate barycentric
interpolation and evaluates the closed-form maximum-roughness metallic BRDF.
It does not read converted importer geometry or replay renderer WGSL.
Every tested precision rejects channel-swap, vertical-flip and missing-material
negative controls over at least 1,000 interior pixels each.

| Precision | Checked interior pixels | Maximum absolute linear error | Allowed error |
| --- | --- | --- | --- |
| Unorm8 | 5256 | 0.00102964787 | 0.002 |
| Unorm16 | 5256 | 8.56496914e-06 | 2e-05 |
| Float16 | 5256 | 6.09303344e-05 | 0.00012 |
| Float32 | 5256 | 2.24169722e-08 | 2e-06 |

Silhouette edges and the triangle tie band are excluded by source coordinates,
not selected from GPU agreement. The clear corner is independently checked.
Original and owned frozen-restored output bytes match exactly in each format
after the original GPU scene owner has been dropped.

## Source-based skeletal animation

SimpleSkin renders at 0, 0.75, 1 and 4 seconds in all four native precisions:
16 actual native frames. This includes quaternion interpolation,
positive and negative rotations, actual skin deformation and exact frozen
restore. The independent Python oracle reads original joints, weights,
inverse binds and animation buffers. Its Z-axis quaternion interpolation and
linear-blend skinning produce reference triangle silhouettes directly.
It never reads deformed renderer/importer vertices. Every nonzero sampled
pose rejects the unanimated bind silhouette by more than 200 checked pixels.
Two source-derived boundary pixels are excluded for raster edge conventions;
interior/background alpha must match exactly. This proves skeletal placement
and coverage, not all possible skin lighting or singular joint configurations.

## Receipts and remaining scope

Source/configuration hashes remain identical to the accepted native aperture
gate. Controller exit codes, exact binary and tool-source hashes, raw native
formats and independently calculated reports are retained in
`data/scene3d-normative-native-v1.zip`. CRC and every archived entry were read back and compared
with the originals. SHA-256: `9a0afdc7acb00b491e4a63adadc5270c058d90477ba24bf68ef70b47c1dbce3f`.

The initial external-probe build selected uncached dependency revisions when
regenerating its separate lock file offline, so it never reached a native
render. That setup failure is retained separately. Seeding the product lock
and matching its existing GPU backends resolved the probe setup; no product
source or expected rendering assertion was changed to make it pass.

The downloaded AlphaBlendModeTest has not yet passed an independent native
reference comparison. Existing analytical alpha fixtures are separate
evidence. Latest-main full-workspace and maintained-desktop cross-platform
CI remain required. This checkpoint alone does not close issue 87 or 82.
