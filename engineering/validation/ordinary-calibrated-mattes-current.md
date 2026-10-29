# Ordinary receivers of calibrated matte sources

This #87 checkpoint extends independent native calibrated matte images to
ordinary receivers. It follows [native source geometry](calibrated-matte-sources-current.md)
and does not close #87 or implement the #82 render/cache runtime.

## Behavior and ownership

Native source projection keeps its own calibrated geometry, camera, masks,
opacity and matte dependencies. An ordinary receiver reads that independent
working-precision color through a private composition-space input. The source's
visible geometry still participates in physical depth; it cannot be replaced
by the combined backdrop or painted over that backdrop as a flat image.

Private input IDs are selected from unoccupied IDs, including original sampled
layers, projected sources and cached model rasters. Selection does not add one
to the largest user ID. A sampled source may have ID `u64::MAX`. Temporary cache
entries have exact map accounting and a scope guard restores previous entries
on success, error and unwind. Effects bindings are invalidated when the inputs
enter and leave the cache. GPU commands retain admitted revisions through the
existing submit/abandonment ledger.

Dependency traversal now follows both ordinary and calibrated parents, rejecting
cycles before projection. An ordinary parent of a calibrated matte therefore
receives the native projection while materializing its own independent image.
Hidden calibrated sources also work without any visible shared-geometry run.
Internal input layers share the existing `MAX_LAYERS` draw capacity; exceeding
that capacity is an explicit `phase=input_alias` refusal, not silent truncation.

## Verification

The first actual GPU control reproduced the old missing composition path for
object 91/source 8. The same independent ray/color oracle then passed without
changing its expected values or tolerance: four precisions, four matte modes,
and original/moved cameras. Ordinary white overlays are compared with the
independently calculated red/blue physical backdrop and the calibrated green
source's footprint and linear luminance.

Additional native tests cover an ordinary/calibrated dependency chain, no
visible calibrated geometry, a maximal user ID, letterboxed output, two pending
frames submitted in reverse, allocation refusal with completed-front preservation,
abandoned-command retirement, eviction/repreparation, and visible sources whose
physical depth lies behind the backdrop. A near-capacity sampled scene must
refuse an extra input and recover on the next valid frame. A real admitted-texture
unit control checks cache restoration and exact accounting after an encode
error and a controlled unwind.

The previous ordinary-receiver blanket rejection control now targets an effected
calibrated source, which still requires an explicit material/depth contract.
Its error and completed-front assertions remain intact; the new native pixel
controls establish the newly supported plain-source behavior.

The source-bound related gate passed 156 distinct tests, zero failed
and zero ignored, across nine native render targets, the temporary-input guard,
compositor, mask GPU and host scheduler. Desktop FFmpeg/AI-depth compilation,
style-consumer and provenance checks passed. Every Rust/configuration source
hash remained unchanged after every step. Focused tests overlap this gate and
are not added to its count. This is local related acceptance, not final full
workspace/current-main or cross-platform acceptance.

## Remaining limits

Calibrated effect owners and explicit effect image inputs, processed visible
ordinary spatial matte sources, nested/exposure depth, intersecting translucent
geometry, DOF, sustained real-codec/P95 measurements and final current-main
regression remain separate work. The ordinary output chain tests are actual GPU
controls, not sustained playback measurements. Android is retired and excluded.
There are no panel/UI, dependency or third-party asset changes.

Receipt: `artifacts/pr-queue/ordinary-calibrated-matte-regression-v1/accepted.json`.
Archive: `data/ordinary-calibrated-mattes-native-v1.zip`; CRC and every entry verified against originals.
SHA-256: `30c3252420ae4c22dd0057a4b5fbd6b9c2772d92791a689a1ab07cb9d82a42a5`.
