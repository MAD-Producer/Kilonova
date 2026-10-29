# Calibrated source-space pointwise materials

This #87 checkpoint adds proven source-space image materials to calibrated
geometry. It extends [ordinary calibrated matte inputs](ordinary-calibrated-mattes-current.md).
It does not close the general effects/depth contract, #87 or the #82 runtime.

## Material and depth contract

Invert and Tint use the existing frozen pointwise implementation proof, checked
against the selected native program. Package/effect names alone do not authorize
the path. A changed compiled implementation, external image input or unproved
operation is refused. Original resolution/program failures retain their causes.
Disabled and zero-output effects do not create an active material boundary.

The source-local image pipeline materializes masks before image effects at the
working precision. It clears the receiver's projected matte only in its private
materialization context. The original calibrated mesh, source coordinates,
sampled world/camera and physical fragment depth remain authoritative. Projected
matte coverage and layer opacity apply afterward. A material whose resulting
plane changes source coordinates is refused; no flat depth is manufactured.

Native calibrated image planes accept an admitted single-sample material in
the current working format in addition to the existing sRGB source uploads.
Ordinary uncalibrated image planes retain the sRGB upload requirement. The same
projective source mapping and interpolation read the material. Source masks
already baked into a material are not multiplied again.

Tint preserves input alpha. Invert RGB/R/G/B modes preserve alpha; Alpha mode
does not, except for an exact identity blend. Only a proven alpha-preserving
material of an opaque, unmasked source can retain opaque depth writes. Other
materials use the existing alpha path and its documented intersection limits.
Material images are immutable per encode, admitted and charged against source
policy as well as their existing exposure/allocation reservations. Pending and
abandoned commands retain their revisions through the normal allocation owner.

The same processed image supplies independently projected calibrated matte
sources. Their visible geometry remains distinct from the color read by an
ordinary or calibrated receiver. Material effects are removed from the final
flattened spatial anchor so they are not applied to the backdrop a second time.

## Verification and alignment

The first native test reproduced the old missing composition path for object 1,
source 7. The unchanged independent world-ray/color oracle then passed in four
precisions, moved/original cameras and reversed authored order. The extended
control also covers a two-stage Invert/Tint chain.

Native controls additionally check mask-before-effect ordering, Alpha inversion
of a masked hole, layer opacity and projected luma matte ordering, processed
calibrated matte sources consumed by both receiver kinds, two pending material
revisions submitted in reverse, typed allocation refusal with front preservation,
abandonment retirement and eviction/repreparation. Synthetic owned video colors
produce independently expected inverse colors at the same PTS without reopening
the missing codec file. Missing owned video pixels must refuse at render admission
even when a previous valid color remains resident. This is not real-codec or
sustained playback/P95 evidence.

CPU proof controls cover sampled alpha modes, identity blends, disabled unknown
effects, explicit external inputs, a replaced native program, original program
failures and recovery. The historical blanket effect-owner rejection control now
uses unproved Fill; its original error/front assertions remain. External image
input rejection remains active. Earlier Fill-source rejection controls also stay.

Own PR #552 was merged only after all five exact-head required checks succeeded.
The actual squash title and body are Chinese, verified from commit
`67760b8b7b2cac07e870454c899f707325abda41`. This branch rebased its nine accepted
mask/matte commits onto that main, dropping the equivalent pre-squash prefix.
The source-bound gate includes the imported main screenshot-failure test and
the full renderer library as well as native scene/composition targets.

The aligned source-bound related gate passed 470 distinct tests, zero failed
and zero ignored: ten native render targets, the full renderer library, host
scheduler and main's screenshot-failure control. Desktop FFmpeg/AI-depth
compilation, style-consumer and provenance checks also passed. Rust/configuration
hashes remained unchanged after every broader step.

The final new camera fixture resets its pose before each material case. Eight
focused native/proof tests then passed on that final source, zero failed/ignored.
Only that new test file differs from the broader gate; production, configuration
and all other test hashes are identical. These eight tests overlap the broader
gate and add zero to its distinct count. Expected ray/color values, tolerances
and assertions remain unchanged. This is related local acceptance on main
`67760b8b7b2cac07e870454c899f707325abda41`, not full-workspace or cross-platform acceptance.

Two earlier new-fixture failures are retained: Alpha mode initially used invalid
channel 0 instead of the published channel 5, and missing owned-video pixels
were initially asserted at normal prefetch rather than actual render admission.
The corrected fixtures retain independent expected colors and original-cause,
object/source and completed-front assertions. These were fixture corrections,
not additional product defect fixes.

## Remaining scope

Other image effects, changed bounds, external effect image reads, processed
ordinary spatial matte sources, nested/exposure depth, intersecting transparency,
DOF, real-codec sustained/P95 and final maintained-desktop full-workspace/CI
acceptance remain open. Android is retired and excluded. There are no panel/UI,
dependency or external asset changes.

Receipts: `artifacts/pr-queue/calibrated-material-regression-v1/accepted.json`
and `artifacts/pr-queue/calibrated-material-camera-v1/accepted.json`.
Archive: `data/calibrated-depth-materials-native-v1.zip`; CRC and every entry verified against originals.
SHA-256: `80b461d1271453ad9b9586f08a88139994f67657b2575a315a3464f5fc19385a`.
