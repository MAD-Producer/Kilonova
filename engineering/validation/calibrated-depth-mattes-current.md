# Calibrated receiver track mattes

Maintained desktop increment for issue #87, following native source masks. This
does not close the issue or establish final full-scene compositing acceptance.

## Behavior

An eligible calibrated image/video plane can consume an ordinary independent
track-matte source. The source is materialized at its effects stage using the
existing retained-layer path, including its own mask/matte dependencies, then
projected into a private transparent working-color image. Source visibility and
layer transfer do not replace its independent matte image.

The calibrated fragment reads that image in native screen coordinates before
physical depth composition. Alpha, inverted alpha, linear luminance and inverted
luminance are explicit pipeline modes. A separate source-UV mask can multiply
this screen coverage; the two coordinate systems are not conflated. The combined
spatial raster removes the consumed mask/matte rather than applying either to
already-combined foreground and background pixels.

Hidden ordinary dependencies remain available to other consumers but no longer
split a spatial run merely because they appear between its visible layers.
Calibrated matte sources remain explicitly rejected: their independent raster
and geometry contract is a separate, unfinished requirement.

## Ownership and budgets

Each encode owns its projected matte targets. Existing retained-layer source and
projection submissions complete their ordering before the caller's shared
geometry commands use them; this is not a single-submission implementation.
New targets join native admission and source-texture budgets. The projected
image is at working precision; float coverage is not baked through RGBA8.
Pipeline keys are bounded independently of asset IDs and animation values.

Extra submissions and per-encode allocations have not received sustained
playback/P95 acceptance. Pooling or submission fusion must preserve immutable
pending commands and independently prove its actual resource retirement.

## Evidence

The unchanged implementation executed the independent native receiver test and
failed with object=1, source=7 and phase=shared_geometry, identifying the missing
depth composition path. This was a real GPU test failure, not a compilation
failure: `artifacts/pr-queue/calibrated-matte-red-v1/expected-red.json`.

Initial implementation controls pass: all four working precisions and four
matte modes; half-alpha green luminance against independent red/blue rays;
effected matte source combined with a source-local mask; opposite pending frames
submitted in reverse order; budget refusal preserving the completed front and
recovery; hidden full-coverage sources before/between/after spatial layers.
An initial implementation type-inference compile error is retained in the v1
log and is not counted as expected failure-path coverage.

The source-bound v4 broader gate passed 142 distinct related tests with zero
failures and zero ignored tests: nine native render integration targets,
compositor and mask-GPU units, and host scheduler tests. Desktop compilation
with FFmpeg/AI depth, style-consumer and provenance checks also pass. Source
hashes remained unchanged after every step. This includes the abandoned-command
front-preservation and reservation-retirement control. This is related local
acceptance, not final umbrella full-workspace/cross-platform acceptance.

## Remaining limits

Visible-source stacking and changing-video matte input remain to verify.
Dedicated source-mask/chained-matte and spatial camera/letterbox controls are
now accepted in the follow-up below. General calibrated-source mattes, calibrated image effects,
nested/exposure depth, intersecting translucent ordering, DOF, long-duration
real-codec throughput and current-main full-workspace/cross-platform acceptance
remain unfinished. Android is retired and excluded by request. No UI/panel,
dependency or third-party asset changes are included.

The broader v1 run exposed an obsolete trailing assertion in the existing
missing/current-depth test: it required every ordinary spatial matte source to
fail. Only that now-supported behavior is changed to the existing independent
world-ray oracle. Pending-depth, source identity, failed-admission preservation
and recovery assertions are unchanged. The original calibrated matte-source
rejection test is also unchanged. The v1 failure is retained.

The calibrated-source eligibility guard is now explicit. A new native regression
first reproduced the incorrect successful flattening when a calibrated source
fed a calibrated receiver; the original failing GPU log is retained separately
from the earlier fixture compile error. The guard must preserve the completed
front and identify object=1/source=7. A visible processed spatial matte source
that cannot join the same physical run also refuses explicitly, with its source
ID, instead of overpainting the shared depth result as an ordinary flat image.
This preserves an unfinished capability as an observable boundary, not delivery
of general processed spatial matte composition.

Receipt: `artifacts/pr-queue/calibrated-matte-regression-v4/accepted.json`.
Archive: `data/calibrated-depth-mattes-native-v1.zip`; every entry was read back against its original.
SHA-256: `42e8e117ee8eb8b914a8ff1177b7c627719da82c81db10817a4413add7555b12`.

The v3 guard regression retained its exact assertions when it exposed a dependent
receiver error being reported before the unsupported calibrated-source error.
All calibrated-source membership checks now run before dependent spatial-boundary
checks, preserving the primary unsupported-source context. The failed v3 log is
retained along with the successful v4 execution.

## Independent context controls

A hidden green source with its own source-local hard mask consumes a second
screen-space matte. Opposite source-mask and inner-matte revisions independently
match the expected linear-luminance blend at both fixed rays. Two pending encodes
submitted in reverse retain the full independently rendered frame for each.
This proves this ordinary-source chain, not arbitrary calibrated-source chains.

A hidden spatial green matte is checked against analytic world-ray/plane
intersections, separately from the calibrated red surface and ordinary blue
plane. Both initial and edited camera/rotated-source scenes use a 192x160 output
with a native 192x128 composition and 16 transparent rows above/below. Every
sampled interior must match its independently computed coverage and physical
depth; each of the three red/matte/blue categories must include at least 30 rays.
The AA/filter boundary is excluded by an explicit two-local-unit margin.

The source-bound context gate passes 144 distinct related tests, zero failures
and zero ignored tests, plus desktop FFmpeg/AI-depth compile, style-consumer and
provenance checks. This repeats the earlier 142 tests and adds two; totals are
not additive across these gates. Production sources are unchanged by these
controls. No UI/panel, dependencies or external assets changed. Final umbrella
full-workspace/cross-platform acceptance remains pending.

Receipt: `artifacts/pr-queue/calibrated-matte-context-v1/accepted.json`.
Archive: `data/calibrated-matte-context-native-v1.zip`; every entry verified against its original.
SHA-256: `984d1f16be95fcc13df83216e36895430036fe38edeab0ce891149cd6b6d2d66`.
