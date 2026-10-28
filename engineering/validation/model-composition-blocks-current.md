# Contiguous model depth inside ordinary 2D compositions

## Result and boundary

Adding an ordinary 2D background or foreground used to disable the all-model
shared pass. Intersecting models then reverted to authored layer order instead
of per-fragment depth, and transparent assets lost global primitive ordering.
Contiguous compatible model runs now share one native pass while surrounding
ordinary 2D layers retain their own authored compositing positions. Each 2D
boundary ends a run; it is never interpreted as a physical 3D surface.

Runs must contain at least two compatible model layers and use one common
camera and sample count. Singleton models and incompatible opacity, tint or
sampling retain the isolated path. Effects, masks, mattes, adjustments, nested
compositions and non-normal blend scenes retain their original receiver path.
True mesh/3D-plane intersections and those image-processing boundaries remain
open. Primitive-center transparency sorting is not exact intersecting
transparency or order-independent blending.

Each run is encoded into the enclosing frame command stream. Its raster is
composited at its original position. The two supported MSAA counts have two
fixed cache slots, so alternating 1x/4x runs reuse admitted depth attachments.
Both slots are included in memory/texture ledgers and cleared on retirement.
The cache key space is fixed, not an unbounded sample-count map.

## Actual acceptance and retained failures

The original parent reproduces both real failures before product edits.
The first outer controller incorrectly expected the left-ray failure message;
the actual parent passed that ray and failed the right ray. Its raw exit and
controller are retained; classification was corrected without rerunning or
changing the test. Initial green passes the mixed-background/transparency case
but exposes a second fixture whose perspective-expanded mesh overlaps its own
separator ray. Moving that fixture then overlaps its own background ray.
Both failed attempts remain recorded. The final fixture shrinks its geometry
to 70 percent while keeping **every original assertion, ray coordinate and
expected value unchanged**. A fresh control runs the final fixtures against
the exact parent product Git blobs: **1 passed, 4 failed, 0 ignored**. Both
original right-ray failures recur, the normal authored-boundary control passes,
and current product bytes are restored and hash-verified afterward.

The final current-source gate executes **392 passed,
zero failed, zero ignored across 11 targets**. Coverage includes the renderer
library, native ScenePass and calibrated depth, all 20 model integrations,
layer compositing/BSP, retained rasters/masks, scene/spatial effects, exposure,
ordinary/nested compositions and multiple-camera compositions. Five new real
GPU regressions cover both authored orders, opaque and transparent materials,
background/foreground pixels, multiple runs, 1x/4x warm-cache identity and
retirement, no merge across a 2D layer, reverse submission of two pending
animated frames, abandoned commands and restored reservation counts.
Provenance, panel-style checks and desktop FFmpeg/AI-depth compilation pass.
The model target's earlier passes overlap this run and are not added.

No existing assertion, tolerance, budget or timeout was relaxed. The earlier
external ONNX calibration checkpoint is separate; inference was not rerun for
this composition-only increment. This is not a new full-workspace or
cross-platform claim. No panels, UI styles, dependencies or third-party assets
changed. Android is retired and excluded; the delivered movie was not rerendered.

## Evidence

[All original red/green attempts, final parent control and current native
gate](data/model-composition-blocks-native-v1.zip): SHA-256 `198af89c3492a488752355b425a602c6da840c446d5e9103291ec6a1fc51535e`,
21,837,195 bytes, 7175 entries. Every entry was
read back and verified against its manifest. Binaries are excluded. Exact
executed product and test inputs are retained in the gates.
