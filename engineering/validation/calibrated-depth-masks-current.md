# Calibrated source-local mask coverage

Scope: the maintained desktop portion of issue #87. This increment follows
PR #552's independent compositing boundaries. It does not close #87 or #82.

## Implementation

- An eligible calibrated image/video plane can carry its own layer masks into
  shared native geometry. Matte, effect-input, nested, adjustment and blend
  boundaries retain their existing explicit restrictions.
- The existing mask planner and GPU raster/feather/combine implementation produce
  scalar coverage at the selected Unorm8, Unorm16, Float16 or Float32 precision.
- Coverage follows the calibrated source UV coordinates, including their
  projective interpolation, before physical depth testing and premultiplied alpha
  composition. It is not applied to the already combined spatial raster.
- Immutable pipeline objects are shared. Mutable mask targets, scratch textures,
  geometry and filter uniforms belong to each encode, preventing a later encode
  or normal 2D preparation from overwriting unsubmitted work. Allocation and
  source-texture budgets remain enforced. Shared raster output removes the
  consumed mask to prevent applying it twice.

## Evidence and limits

The original implementation genuinely fails the new GPU test with object=1,
source=7 and phase=shared_geometry, reporting the missing depth composition path.
Receipt: `artifacts/pr-queue/calibrated-mask-red-v1/expected-red.json`.
The initial new fixture used decoded 2x1 texel coordinates; inspection of
`scene.rs` established that masks use the authored 64x40 source rectangle.
The fixture was corrected before validating the new implementation. The original
failing log is retained; no existing assertions were changed. The broader v1
gate also caught a new fixture unit mistake: authored mask opacity is a percent,
so 0.5 means 0.5%, not 50%. The fixture now authors 50 while retaining the same
independent half-linear-color expectation; the failed v1 receipt is retained.

Initial native validation passed two tests, including four working precisions,
mask inversion and restoration, and independently encoded feather/opacity changes
submitted in reverse order. Budget refusal preserves the completed front;
restoring the budget recovers rendering; abandoning commands preserves the front
and releases reservations after pending writes retire. A subsequent broader gate
also checks half-coverage linear red over opaque blue before SDR conversion.

Broader source-bound validation passed 136 distinct relevant tests,
with zero failures or ignored tests. It includes nine render integrations, mask
GPU and compositor units, the host scheduler, desktop compilation with FFmpeg
and AI depth, style-consumer checks and provenance validation. This is relevant
local acceptance, not final umbrella full-workspace or cross-platform proof.
Receipt: `artifacts/pr-queue/calibrated-mask-regression-v2/accepted.json`.

Archive: `data/calibrated-depth-masks-native-v1.zip`. Every archived entry was read back and compared
with its original. SHA-256: `0aedd6e9fedb0f67d62d7cf41d891c57689580bc81a87e29d0a6c714777acc7f`.

The native mask fixtures use a static owned source; the same plane binding accepts
owned video, but a dedicated changing-video-with-masks native fixture remains
pending and is not established by these static tests.

This increment does not establish sustained playback throughput or a performance
budget. Per-encode mutable resources deliberately prioritize immutable commands;
pooling would need ownership-aware reuse and its own measurements. Existing
intersecting translucent primitive-order limitations remain. Calibrated mattes,
effects, nesting, exposure integration, DOF and final current-main/full-workspace
and cross-platform acceptance remain open. No UI or panel code, dependencies or
third-party assets are changed. Android validation remains excluded by request.
