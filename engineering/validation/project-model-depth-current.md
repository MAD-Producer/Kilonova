# Project model shared-depth checkpoint

Status: the actual project renderer now consumes the native shared mesh pass
for homogeneous model compositions. Targeted native acceptance passes;
cross-platform merge-ref CI remains separate. This does not close #87.

## Behavior and functional boundaries

Previously each model layer rasterized into a separately cleared depth target.
Layer compositing then selected an authored order even when two model surfaces
intersected, and transparent materials sorted only within their own source.
Two actual project regressions reproduce both defects on unchanged code:
zero passed, two failed, zero ignored.

For plain all-model compositions, one common camera, depth attachment and
explicit common 1x/4x sample policy now cover every source and instance. Opaque
and masked surfaces use per-fragment depth; transparent primitives use the
native pass's existing global back-to-front queue. The resulting composition
raster is composited once. The original sampled scene and saved authoring are
retained for editing and independent per-layer receivers.

Native draws and the final raster are recorded in the same enclosing frame
encoder. There is no intermediate submission and no mutable frame uniform
reuse. The public allocated native API retains its encoder-owner check; the
new internal bridge uses the renderer's existing encoder/allocation scope.
Pass attachments join model texture statistics and the deduplicated resource
ledger. Source-pool admission includes all retained model material pixels and
the combined raster, with typed original causes and replacement rollback.

This first consumer requires at least two model layers, matching authored
sample counts, full layer opacity, default sampling and ordinary compositing.
Plane layers, effects, masks, mattes, nested compositions, adjustment layers,
nonstandard blends and differing sample counts retain the existing isolated
path. They are not silently reinterpreted as shared physical geometry. Global
transparent primitive ordering does not claim exact intersecting-transparency
or order-independent-transparency support.

## Native acceptance and reference correction

Five new regressions verify saved/restored project rendering after model files
are removed, both authored layer orders, two independent material sources,
opaque intersections, global transparency, reverse submission, abandoned frame
lease retirement, an exact one-byte source-raster refusal with unchanged front
pixels and recovery, and deletion of the last model's depth resources. They
also exercise all four working precisions with both sample counts and verify
the actual storage formats. The original two red assertions remain unchanged.

The first expanded gate encounters two compile-only fixture mistakes: the raw
readback helper initially accesses a private field, then a private accessor.
Both attempts retain their exact source and run no GPU tests. The final helper
uses the existing public texture view; no product API is exposed for the test.

Native-v4 executes the renderer library, model integration and all native scene
tests. The 274 library tests and all 45 scene tests pass these exact production
bytes. Fourteen model tests pass, while a newly authored ideal-bit reference
fails for Float16: the red channel is one representable unit below ideal one.
The unchanged isolated one-model path independently produces those same bytes.
That ideal-color assumption is therefore not used to diagnose shared-depth
regressions. The final reference selects the expected blue and red rays from
independent isolated renders at the same precision. The comparison remains
bit-exact, and explicit storage and nonempty-reference checks are added; no
tolerance, budget, timeout or existing assertion is weakened.

Native-v5 reruns the entire model target: **15 passed, zero failed or ignored**.
Eight raw reference reports retain both ideal and actual isolated bytes. The
controller verifies the original Float16 failure is independently reproduced by
the isolated reference, every production hash is unchanged, and the original
bit-equality assertion is unchanged. Non-overlapping accepted coverage is
**319 + 15 = 334 tests across three targets**, zero unresolved failures or
ignores. This is an explicit two-stage closure, not a claim the partial earlier
run was green. Provenance, panel-style checks and FFmpeg/AI-depth desktop
compilation also pass. Existing compiler warnings remain.

[Raw red/green outputs, historical failed attempts, exact source and precision
controls](data/project-model-depth-native-v1.zip): SHA-256
`19bd85284711068823a44a5ce1039ee20277710d6aba21d3d32f1b4df8ed8699`,
737,602 bytes, 83 entries, read back and verified. Binaries are excluded.

No panels, styles, dependencies or third-party assets are changed, and the
delivered movie is not rerendered. Android is retired and excluded. Mixed
project model/plane/matte/effect/nested depth, persisted authored calibration,
current-frame depth routing, arbitrary image-plane reprojection and imported
camera selection remain open. The separate calibration and parent full
workspace gates cannot substitute for this later consumer's acceptance.


The later [final combined checkpoint](depth-project-final-current.md) rebuilds
the real calibrated inference harness after the project shared consumer and
records its exact final source and scope. Earlier gates are not added together.
