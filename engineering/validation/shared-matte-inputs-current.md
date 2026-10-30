# Shared independent matte inputs for ordinary and calibrated receivers

This #87 checkpoint extends [ordinary spatial matte materials](spatial-matte-materials-current.md)
from exact accepted parent `52ff184ec3d663f41e5ae06b768f41286cbe8277`.
It removes the exclusively-calibrated-consumer restriction for the supported
visible ordinary image-backed material sources. #87 and the #82 runtime remain
open; this is not a final scene/DOF or prerender-cache completion claim.

## Independent reads and visible geometry

The existing bounded matte dependency traversal now starts at calibrated
receivers before material eligibility. Ordinary intermediate matte parents can
therefore lead to a visible spatial image source. The source uses its existing
implementation-proven pointwise material and local masks; an ordinary consumer
no longer makes that source ineligible for native geometry.

Every needed independently projected matte can receive a private composition-
space alias, including ordinary spatial sources and ordinary intermediate
parents. The alias has no visible contribution, source transforms, opacity,
masks, effects or matte chain of its own. Those are already represented in its
retained projected pixels. The original source's visible contribution remains
separate and retains its actual physical depth in the shared run.

Source-space material textures are reused directly for native planar projection,
instead of materializing the same ordinary effects chain a second time. The
projection still uses the original plane/camera. Across a real authored 2D
boundary, an ungrouped visible source retains its own effects in the authored
output; only effects baked into shared geometry, or hidden independently read
sources, are stripped. A 2D boundary does not silently erase the source material.

The existing alias guard restores temporary cache entries and exact byte
accounting on success, error and unwind. Submitted/abandoned encoders retain the
admitted pixel revisions. Input count, source/global allocation budgets and ID
selection remain bounded; maximal user IDs do not become alias arithmetic.
Raw upload/working-material formats, implementation proof, alpha policy and
source-coordinate validation retain the previous contracts.

## Native evidence

The first actual GPU control reproduced the old object 1/source 7 refusal for
visible source 91 when a white ordinary receiver shared the matte. The same
independent linear-color expectations and tolerance passed after the change.

Three new native tests cover:

- All four precisions and all four matte modes; independent half-opacity
  magenta luma and ordinary white-over-physical-background arithmetic; masked
  exterior rays and physical-layer authored-order reversal
- An ordinary intermediate parent with its own half opacity, shared direct and
  indirect consumers, an authored off-screen 2D boundary and letterboxed output;
  the boundary control detects accidental removal of visible source effects
- A maximal user source ID, two pending mask revisions submitted in reverse,
  typed admission refusal, unchanged completed fronts, abandonment retirement
  with exact admitted bytes, and eviction/repreparation

The earlier blanket shared-ordinary-consumer refusal fixture now adds a real
explicit external displacement image input, which remains unsupported. Its
original cause/object/source/front/recovery assertions are unchanged. The new
positive pixel tests prove the newly supported shared-consumer contract; this
fixture change does not claim external effect image reads are implemented.
The separate unproved Fill refusal remains unchanged.

The source-bound related gate passed 476 distinct tests, zero failed
and zero ignored: ten native render targets, the full renderer library, host
scheduler and main screenshot-failure control. Desktop FFmpeg/AI-depth
compilation, style-consumer and provenance checks also passed. Every tracked or
new Rust/configuration hash remained unchanged after every step. Focused tests
overlap the gate and are not additional distinct coverage. This is related
local acceptance on the stated parent plus hashed changes, not final full-
workspace or cross-platform acceptance.

## Remaining scope

Explicit source/effects-stage external image inputs, non-image processed spatial
sources, general effects/changed bounds, source matte chains beyond the stated
material eligibility, nested/exposure depth, intersecting transparency, DOF,
real-codec sustained playback/P95, final maintained-desktop regression/CI and
actual #82 prerender/cache integration remain open. Android is retired and
excluded. No UI/panel, dependency or third-party asset changes are included.

Receipt: `artifacts/pr-queue/shared-matte-input-regression-v1/accepted.json`.
Archive: `data/shared-matte-inputs-native-v1.zip`; CRC and every entry verified against originals.
SHA-256: `937b6243234356a401ae2faffd208bbebcb7d07631f9ba7d2b6e72213974c2c6`.
