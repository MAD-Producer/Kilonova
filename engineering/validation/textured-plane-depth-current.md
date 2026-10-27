# Opaque static-image planes in native shared geometry depth

## Behavior and ownership

This increment is based on the exact published PR528 head
`2d4553077d00f98047177453c0a3cabb2e85078f`. Contiguous meshes, solid planes,
and proven opaque static-image planes use one camera and hardware fragment
depth. Image UVs use perspective interpolation with authored image Y direction.
Layer order no longer replaces physical occlusion for these compatible runs.
Pure-plane scenes retain the original BSP path.

Opacity is recorded once from actual uploaded alpha bytes, with no per-frame
readback or image scan. A successful native image-plane encode freezes that
source revision. Its next upload admits a replacement texture before changing
the cache. Existing allocation-encoder reservations retain earlier revisions
until submission or abandonment, so queued frames keep their original pixels.
There is no per-frame texture copy or unbounded retired-image map. A revision
stays frozen until replaced even after GPU completion; the extra allocation
occurs on a later source upload, never on a healthy unchanged frame.

Solid vertices retain their existing 192-byte per-plane contract. Textured
vertices require 240 bytes. The pipeline cache has at most 16 keys: four working
formats, two sample counts, and two source kinds. Image IDs and animation do not
add cache keys. Generated images, vectors, video, effects and masks receive no
opaque-source fact by inference.

## Actual acceptance

The unchanged parent product fails both original GPU regressions: an image
covers the nearer mesh, and an in-place upload changes an already encoded
frame. The initial green attempt fails compilation on a missing generated-image
metadata initializer and temporary vertex attributes. Product compilation is
corrected; neither original regression test nor its assertions are changed.
The same two GPU tests then pass. Expanded coverage appends another module.
Its first run has 412 passes and one failure in the new retirement fixture:
it wrongly classifies the permanent white texture as a static image. The
existing breakdown explicitly records it as a system texture. Only that new
fixture is corrected to compare both categories against its actual initial
empty-catalog baseline. Product code, budgets, tolerances, original regressions
and pre-existing assertions remain unchanged; the failure history is retained.

Final source-bound acceptance has **413 passed, 0 failed, 0 ignored**
across 13 native targets: the 12 successful unchanged targets
(382 tests) are retained, and the complete corrected model target reruns with
31 passes. Source hashes prove the only intervening change is that new fixture.
Provenance, panel-style validation and desktop FFmpeg/AI-depth compilation
also pass. These are combined source-bound results, not one all-green original
controller execution. The six new native tests
cover saved/reopened real PNGs with deleted source files, both authored orders,
reverse submission and abandonment, all four working precisions at 1x/4x MSAA,
independent f64 camera/parent rays and multiple image regions, transparent-source
fallback and opaque recovery, exact one-byte-short texture/vertex refusals,
unchanged output after failure, successful recovery and deletion retirement.
Existing renderer, scene, mask, matte, composition, effect, exposure and nested
regressions are included as appropriate. Overlapping red/green counts are not
added to this final total.

## Remaining boundaries

This implements opaque static-image planes. Transparent planes still use the
existing independent composition path; their depth behavior is not claimed as
completed. Effects, masks, mattes, nesting, nonnormal blends and incompatible
sampling retain existing boundaries. Imported-camera project selection and
tracks, persisted current-frame depth calibration/reprojection and DOF remain
open with #74/#87. These tests do not constitute new full-workspace, GUI or
cross-platform acceptance. PR528's CI belongs to its own published head.

No UI/panel/style changes, dependencies or third-party assets are added.
Android is retired and excluded. Existing assertions, tolerances, budgets,
timeouts and ignored-test policy are unchanged. #74 and #87 remain open.

## Evidence

[Source-bound failures and final acceptance](data/textured-plane-depth-native-v1.zip): SHA-256 `82877486bb84a364e23d4e4399844805dfbce4e70fd21313221e2c308bbcd958`,
920220 bytes, 79 entries. Every entry is read
back and hash-verified. Only changed source is copied; complete source hashes
identify the executed baseline. Binaries and shared libraries are excluded.
