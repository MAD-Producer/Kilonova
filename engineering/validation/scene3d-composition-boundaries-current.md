# Native 3D composition boundary inventory

This inventory follows issue 87's mixed 2D/3D, parent/camera/matte and existing
BSP requirements. It distinguishes actual pixel behavior from explicit refusal.
It does not turn a documented refusal into support for the refused operation.
The maintained acceptance scope is desktop; Android is retired and excluded.
The [original-scope disposition](scene3d-composition-disposition-current.md)
binds nineteen existing controls to actual native/CI execution, including
independent ordinary BSP regressions. It reconciles this inventory without
claiming universal compositor support or adding overlapping test counts.

Source boundary: `crates/motion-render/src/renderer/model_layers/shared.rs`,
especially `samples`, `shared_plane`, `native_matte_sources`, `blocks` and
`prepare_shared_model_scene`. A shared run owns geometry depth. Independent
image consumers must keep their own source/effects-stage rasters. Combining
their inputs into the run's already-composited image would change semantics.

## Implemented behavior and corresponding controls

| Combination | Current behavior | Actual control location |
| --- | --- | --- |
| Pure ordinary 3D planes, aperture disabled | Existing BSP path is retained; shared mesh rasterization is not substituted | `tests/layer_compositing.rs`; `tests/model_layers/mixed_planes.rs` |
| Eligible meshes and ordinary solid/image planes | Common camera, per-fragment depth, explicit 1/4 samples, authored 2D boundaries | `tests/model_layers/project_shared.rs`, `mixed_planes.rs`, `textured_planes.rs` and `transparent_planes.rs` |
| Calibrated image/video surface and ordinary spatial backdrop | Calibrated camera-space geometry, original-source identity and actual fragment depth | `tests/depth_surface.rs`, `depth_surface/video.rs`, `masked_video.rs` |
| Unrelated 2D mask receiver | Its own coverage remains independent of the shared depth run | `composition_boundaries::unrelated_masked_2d_overlay_preserves_shared_depth_and_its_own_coverage` |
| Unrelated 2D matte receiver | Matte source and target stay independent; shared depth is preserved | `composition_boundaries::unrelated_matte_2d_overlay_keeps_source_and_depth_rasters_independent` |
| Unrelated 2D effects / external image-input DAG | Independent source and effects stages remain distinct from the depth raster | `composition_effects::unrelated_image_effect_keeps_its_pixels_without_disabling_calibrated_depth`, `unrelated_external_image_inputs_keep_source_and_effects_stages_independent` |
| Unrelated non-normal 2D blend | Authored stacking and shared-depth pixels are retained in both color spaces | `composition_blend::unrelated_blend_preserves_depth_and_authored_order_in_both_color_spaces` |
| Calibrated masks | Coverage is applied to the source before physical depth; immutable pending revisions and bounded admission | `calibrated_masks.rs`, `masked_video.rs` |
| Calibrated matte receiver | Alpha/luma/inverted modes project source coverage in the correct camera; hidden sources do not split physical depth | `calibrated_mattes.rs` |
| Ordinary receiver reading calibrated matte | Independent projection retains physical geometry and source identity after camera edits | `ordinary_calibrated_mattes.rs` |
| Proven pointwise calibrated materials | Supported processed color precedes spatial depth; masks, matte and opacity retain their specified ordering | `calibrated_materials.rs`, `spatial_matte_materials.rs` |
| Shared ordinary matte image | Ordinary and calibrated consumers receive independent aliases and exact processed inputs; visible source depth is retained | `shared_matte_inputs.rs` |
| Imported model/camera in parent rigs and nested composition clocks | Native source camera, rig and clip clock are sampled explicitly; legacy authored Euler tracks remain separate | `tests/model_layers/imported_cameras.rs` |
| Model exposure receiver | Each shutter pose is materialized independently; average matches independent native samples | `tests/model_layers/exposure.rs` |
| Eligible opaque/masked geometry with native perspective aperture | Actual depth drives animated focus; unrelated 2D boundary stays sharp; immutable parameters and scratch ownership | `tests/scene_gpu/depth_of_field.rs`, `tests/depth_surface/depth_of_field.rs`, `tests/model_layers/depth_of_field.rs` |

Paths in the last column are relative to `crates/motion-render`. A test name is
a navigation reference; its execution must be established by the source-bound
gate, rather than inferred from its presence in the tree.

## Explicit limits, not implemented combinations

| Combination | Disposition and reason | Remaining work |
| --- | --- | --- |
| Calibrated source with unproven material/external input | Refused before replacing completed output; a flat image is not substituted for displaced geometry | Prove an explicit material/input contract before widening admission |
| Calibrated source with non-normal spatial blend | Explicit refusal; an unrelated 2D blend is supported separately | Define and implement geometry-aware blend ordering |
| Calibrated shared run with nested composition, adjustment layer or scene-level effect | `blocks` refuses a shared run; the calibrated layer then returns an object/source/phase error | Define the shared camera/depth/material contract across those boundaries |
| Calibrated exposure / non-default layer sampling | No claim of support; default shared-plane admission is required | Bind each actual source-time depth/color sample to each shutter pose |
| General intersecting translucent surfaces | Sorted alpha profile, not order-independent transparency | Keep this limitation visible; do not claim arbitrary intersections are solved |
| Positive aperture with blended geometry or orthographic camera | Explicit native aperture refusal | No implicit pinhole fallback or silent removal of aperture |
| Positive aperture in packed/legacy plane consumers, including nested aperture | Explicit refusal before caller-buffer writes; zero aperture recovers | Native rendering or an explicit user-requested bake is required |
| Source combinations outside the proven mask/matte/material profile | Existing explicit refusal and completed-front preservation | New functionality needs its own geometric and independent-pixel evidence |

Several limits are larger capabilities than issue 87's initial lightweight
scene profile. They remain visible here so closing a bounded milestone cannot
be interpreted as universal compositor support. Any limit that contradicts a
specific promised acceptance case must be implemented, not relabeled complete.

## Acceptance status

The source-bound aperture gate passed 1041 distinct related tests, zero
failures and zero unexecuted ignored controls. Its ten actual rendering targets
passed 191 tests. Receipt:
`artifacts/pr-queue/native-camera-dof-regression-v2/accepted.json`.
Official Box, SimpleSkin and AlphaBlendModeTest rendering and the measured
60-second native real-codec host run have separate accepted checkpoints.
The [independent alpha comparison](scene3d-normative-alpha-native-current.md)
does not establish arbitrary intersecting transparency. Latest-main
full-workspace / maintained-desktop cross-platform CI remain open. This
inventory alone does not close issue 87 or background
prerender issue 82.
