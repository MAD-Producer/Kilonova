# Native calibrated image depth acceptance

## Accepted source and scope

Implementation `a8f95c8accf873f25b566fadf6a469af573d8dd6` is integrated with main `3ec17f5e427810165b05c0ae29a3282cd9dc0b34`.
The source-bound affected-suite gate passes **1069 Rust tests**, with no
failures, plus **13 Python workflow tests**. Two pre-existing, explicitly manual
large-file media acceptances remain unexecuted; they are not counted as passing.
No existing product-test assertion or threshold was weakened. This is an image
depth increment, not final workspace or cross-platform #87 acceptance.

| Suite | Passed | Failed | Unexecuted manual tests |
| --- | ---: | ---: | ---: |
| media | 169 | 0 | 2 |
| scene | 85 | 0 | 0 |
| model | 101 | 0 | 0 |
| core | 96 | 0 | 0 |
| native | 457 | 0 | 0 |
| host | 161 | 0 | 0 |

Depth-disabled host compilation, desktop FFmpeg/AI-depth compilation, provenance
and panel-style checks also pass. Each step verifies all captured source hashes.
This evidence includes incoming project-library source, not an old pre-merge tree.

## Actual behavior verified

- Saved calibration captures the authored reference camera, world matrix and
  plane size. The free observation camera does not become the saved source camera.
- Native preview/capture reconstructs a bounded triangle surface in layer-local
  coordinates. Source crop, quarter-turn and mirroring are explicit; unknown or
  invalid vertices retain flat positions. Triangles interpolate between vertices;
  this is not a claim that every unknown-region interior remains exactly flat.
- A standalone analytic ray/surface intersection oracle checks opaque occlusion
  after camera and parent edits at Unorm8, Unorm16, Float16 and Float32. Reversed
  layer submission preserves pixels. Exact opaque coplanar ties use object IDs;
  authored transparent ordering and the existing pure-plane BSP path remain.
- Layer width/height edits scale local X/Y against the saved reference extent.
  They do not rescale calibrated metric Z; ordinary layer/parent transforms still
  affect XYZ. Both resized shapes are ray-checked at all four precisions.
- Homogeneous texture coordinates preserve the source photograph in its reference
  camera. Manual floating-point bilinear sampling matches the ordinary perspective
  plane within one SDR code value at the checked interior samples of a nonuniform gradient.
- Geometry cache entries include an opaque immutable resource identity. A new
  prediction with identical source revision, input fingerprint and interpretation
  cannot reuse an old surface. Clones keep identity; tokens retain no scalar plane.
- Pending, canceled or stale session results cannot render/export a misleading
  flat frame. Real host PNG pixels demonstrate near/far occlusion; changing frame
  requires a fresh current request. A known-good front survives refusal, restored
  capacity recovers, and abandoned/reversed pending submissions retain ownership.
- Unsupported portable plan/geometry outputs refuse before modifying caller buffers.

The archive retains 18 original generated fixture PNGs: reference and moved-camera
views plus two resized shapes at all four precisions, and flat/depth photo pairs.
The PNGs are SDR observations of high-precision working renders, not high-bit-depth
PNG export claims. Fixtures and shaders are independently authored; no new external
assets, dependencies, UI, styles or interaction changes are included.

## Negative checks and evidence limits

The new replacement regression first failed against the previous cache: metadata
matched while depth samples changed, so old geometry was reused. Its raw failure
and command receipt are archived. The corrected six-test native suite passes.
One merge invocation accidentally attempted compilation with unresolved duplicate
authoring changes; the compiler failure is retained separately, not as a product
test result. The merge resolution was checked to preserve the entire accepted tree.

An earlier broad invocation stopped after the media step because its controller
incorrectly required zero ignored tests for a suite containing two existing manual
large-file acceptances. The final controller verifies their exact names/reasons;
neither was run or counted. Every other selected Rust suite has zero ignored tests.
Earlier focused bring-up also observed one native access violation (0xc0000005)
without a diagnostic stack. It did not recur in the later serial gates; its root
cause is unproven, so these passes are not a claim that this crash was fixed.

## Remaining #87 work

Publication also integrates main `8c271bd3` (desktop audio preview). The six
model/render/host/core/scene/media packages are byte-identical to the accepted
native source above. Required PR CI validates this later desktop integration;
the local counts above do not claim its additional desktop tests ran in that gate.

Calibrated video/multiple-source routing, complex effect/mask/matte/nested/exposure
depth, intersecting transparency, DOF, encoded frozen-camera video acceptance,
normative comparisons and final current-main full regression/cross-platform CI
remain open. Current static-image routing requires explicit backend configuration
and current-source requests; no implicit inference retry or worker restart is added.
Portable consumers require native support or explicit baking. Android is retired
and excluded; later optional scene-node/RT/volume work is outside this increment.

[Exact-source evidence](data/depth-surface-native-v4.zip), SHA-256
`85174f9d3a57066cd1422f53c3107c65f29cacc57b1dfb28778c462e6000746c`, 578597 bytes. All archive entries were read back
and hash-verified. Later documentation commits do not change the tested source.
