# Chroma-key compositing API example

This independently authored MIT example is an ordinary SDK image effect. It
demonstrates a chroma key and dominant-channel spill suppression using the
existing package loader, effect parameter tracks and host editing APIs. It is
not installed into the default effect catalogue and adds no product dependency,
panel, native editor, artwork or third-party source. The synthetic native
control passes. A photographed workflow using an explicit manual protection
mask also passes its core/history/export controls; the unassisted classifier
still fails two dark foreground pixels in frame 502. This work does not close issue 86. See the
[current validation record](../../../engineering/validation/professional-key86-current.md).

## Contract

The pass requests straight linear RGB and alpha through the existing color
contract. For classification only, negative RGB components become zero and the
remaining components are normalized to sum to one. Division by the largest
positive component precedes summation to avoid overflow on finite Float32 HDR
highlights. Euclidean chroma distance from the selected key determines coverage.
The key color's alpha is unused.

- Distance at or below `tolerance` produces zero coverage
- Positive `softness` gives a smooth cubic transition to full coverage at
  `tolerance + softness`; zero softness selects a hard threshold explicitly
- Source alpha multiplies coverage; transparent results have zero RGBA
- An all-nonpositive source retains coverage, preserving black/negative imagery
- A black key is an explicit identity operation, because it has no chroma
  direction
- `spill` removes a fraction of the surviving RGB's positive dominant-channel
  excess over the larger of zero and the other channels; ties prefer green,
  then blue, then red

Classification does not clamp output RGB. With spill disabled, surviving
negative and above-one values are preserved; alpha handling follows the host's
straight/premultiplied conversions. UNORM output still has its declared bounded
precision. This does not add HDR file import, a tone mapper or an output policy.

`key_color`, `tolerance`, `softness` and `spill` are four existing numeric/color
parameter types and may use the existing animation/expression contract. A single
frame and source-space image are sufficient; no temporal sampling is requested.
Choose key color in the declared linear space, rather than copying display RGB
bytes into a linear color parameter.

## Build and use

Build the existing `effect_tool`, then pack this directory with the existing SDK
packer. Resolve the package's actual hash and effect definition, construct an
`EffectInstance`, and insert it through the ordinary `effect` command. The public
parameter-animation example in
[`effect-parameter-animation.md`](../../../engineering/host-api/effect-parameter-animation.md)
describes atomic command batches, clip-local time, history and persistence.
Keep the package bytes with the project so a fresh Session resolves the same
implementation. No compatibility name or external binary is required.

## Limits to verify with actual footage

This is a simple chroma classifier. A green/blue foreground, colored shadows,
lighting gradients, hair, motion blur and reflections can require garbage/holdout
mattes and additional processing. Dominant-channel suppression can alter a
legitimate foreground color; it does not reconstruct the original illumination.
It does not infer screen color, estimate a clean plate, track or propagate a
matte, perform temporal filtering or recover a physical ground-truth alpha.

Shader/CPU agreement proves numerical execution of this contract, not matte
quality. Photographed acceptance must retain source attribution and hashes,
declared input/output conversions, independently labelled foreground/background
regions, edge crops, actual parameters/commands, preview/frozen output and
history/reopen comparisons. Neither a synthetic green card nor three downloaded
frames alone establishes a completed professional shot.

## Reproduce the native controls

Run `cargo test -p motion-desktop-media --test professional_key -- --test-threads=1`
with the default GPU test feature and an actual supported adapter. The numerical
control exercises clip-local spill animation at fractional times against a
separately written f64 oracle. The photographed control uses three bundled,
attributed SDR derivatives of numbered Blender Foundation / Mango frames; see
[`source.json`](../../../crates/motion-desktop-media/tests/fixtures/chroma-key/source.json)
and its adjacent license for original identities and the explicit conversion.

The photographed preset is selected from frame 500 before native execution and
then held fixed for frames 501 and 502. Manually labelled opaque foreground and
background rectangles have fixed alpha budgets of at least 250 and at most 5,
respectively, on an 8-bit scale. Both test compositions explicitly have a
transparent background so those controls measure keyed-layer coverage rather
than the opaque background used by the project constructor. These rectangles
test core classification;
person, boots and rope crops require separate visual inspection and do not have
physical ground-truth mattes. The three stills use an authored 24 fps demo clock;
the original capture frame rate and PTS are unknown.

Both controls exercise public atomic edits, undo/redo, save and fresh Session
reopen. Frozen PNG export must match preview after removing live layers,
closing Sessions and deleting original image/package files. Set
`MOTION_PROFESSIONAL_KEY_ARTIFACTS` to an absolute output directory to retain
source identities, commands, projects, preview/golden images, alpha mattes,
edge crops, frozen PNG and export receipt. Test source and packaging validation
alone are not evidence that these runtime assertions passed.

## Explicit manual protection workflow

The photographed test duplicates the source above its keyed layer and adds a
nine-vertex inner-clothing source mask with one-pixel feather through public
`add` and `mask` commands. Vertices and the complete command list are retained.
This is a manual compositing operation authored after observing the raw failure,
not an automatic key improvement or blind qualification. The original preset,
core labels and alpha budgets remain fixed. Unprotected output is retained
separately, including the known two-pixel failure. Protected RGB is unkeyed and
can retain local spill. No tracking or full-shot edge quality is claimed.
