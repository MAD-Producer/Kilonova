# Issue 86 chroma-key SDK sample: partial native validation

The SDK sample is not installed into the product catalogue. The synthetic
control and the photographed workflow with an explicitly authored manual
holdout pass their native checks. The raw automatic classifier still fails
two dark foreground pixels. Neither issue 86 nor the full 3D route is complete.
No panel, production behavior or dependency changes are included. Original
core regions, alpha floors and numerical budgets are retained.

## Actual native execution

The source baseline is `eeada782742cae29935f59b9d89ee5145b92712a` with the new
SDK sample and tests. The dedicated target directory belongs only to this
worktree. The original failure executable SHA-256 is
`0be49c14e59da9e5e506ba8722c7bf521b8ed9a438f573cb031a2a29862e9745`.
The manual-workflow executable is
`3291d0f50676d714d50800efd03b8b723260e8cc1ef44f9315b6016d626bb536`.
It runs both tests with the default GPU feature and one test thread. Source
hashes and executable hashes are checked before and after execution. Default
Info, the original backend and stack settings, and controlled cache/temp paths
are retained.

The original first invocation failed both controls because the new fixtures
inherited the project constructor's opaque composition background. A keyed
transparent layer composited over that background still has alpha 255. Both
new projects now explicitly use transparent backgrounds. No alpha, numerical
or image-comparison threshold was changed to make this correction.

The corrected synthetic test passes all five sampled times, local spill
animation, the independent f64 oracle, atomic undo/redo, save and a fresh
Session reopen, and frozen PNG output after live layers, Sessions, source
images and the package are removed. Maximum byte errors against its oracle
are 1, 2, 1, 0 and 1 at frames 5, 7.5, 15, 0 and 10, within the original budget
of 3. Frozen output exactly matches the fractional-time preview. Native and
oracle images were inspected over the same diagnostic checkerboard.

## Photographed failure retained

The fixed preset is selected from frame 500 and then held unchanged for
501 and 502. Sources, licenses, original/derived hashes, color conversion,
manual core rectangles and original budgets are in
[`source.json`](../../crates/motion-desktop-media/tests/fixtures/chroma-key/source.json).
The source capture rate and PTS are unknown; the demo's 24 fps clock is authored.
These are clipped 8-bit SDR derivatives, not a native HDR import/export proof.

All labelled rectangles pass for frames 500 and 501. For frame 502, five
rectangles pass; two of the 840 pixels in `front-trousers` violate the original
foreground alpha floor of 250. At source coordinate (325, 230), RGB [1, 7, 2]
is classified as screen color and becomes transparent. At (324, 237), RGB
[4, 10, 4] receives alpha 227. The label rectangles and budgets remain unchanged.
The source and actual output were inspected; the rectangles cover dark
foreground clothing. Normalizing a very dark green pixel's chroma can make
it indistinguishable from the selected screen direction. This is a limitation
of this simple classifier, not proof of a host API defect.

The original unassisted failure remains reproducible in the current executable:
all three raw-key images are retained separately before restoring the authored
holdout. This is not an automatic classifier repair.

## Authored manual holdout: native verification

A broad nine-vertex polygon follows the visible interior of the front person's
dark clothing, with a one-pixel feather. It was authored on frame 500 **after**
the raw failure was known, then kept fixed across all three frames. These
frames are not an unseen/blind qualification of that manual work. Vertices,
authoring scope and limitations are explicit in `source.json`; no original
core rectangle, alpha threshold, shader or preset was changed.

The existing public API creates a duplicate source layer above each keyed
layer, adds a source mask and keeps its original RGB. This protected area may
retain original spill; it is not a recovered clean foreground color. Removing
the three protection layers through one public edit records the raw output;
undo restores the exact assisted project. A static interior holdout is not
tracking, automatic roto, a garbage matte or a complete shot cleanup.

Native v4 runs both GPU tests: **2 passed, 0 failed, 0 ignored**, 23.33 seconds
of test execution. All 18 original region/frame checks pass in the assisted
output: labelled background alpha is 0 and labelled foreground alpha is 255.
The unassisted frame-502 trousers still contain the same two failing pixels.
All three before/after-spill comparisons, fractional-time still hold, atomic
undo/redo, save and fresh Session reopen pass. Actual frozen PNG is byte-exact
to frame-1.5 preview after deleting all six live layers, closing both Sessions
and removing original sources/package. This photographed persistence/export
proof is new; it is not inferred from the earlier synthetic success.

The complete three-frame source/raw-key/assisted-key/alpha contact sheet was
inspected at 1:1. Core controls are useful but not physical soft-edge alpha
truth, full-shot temporal acceptance or native HDR input/output qualification.

## Local evidence

The retained run directories under `E:/Dev/aem/artifacts/pr-queue/` are:

- `professional86-key-native-v1`: original opaque-background failures
- `professional86-key-link-v2` and `professional86-key-native-v2`: isolated link,
  synthetic success and original frame-502 foreground failure
- `professional86-key-link-v3` and `professional86-key-native-v3`: unchanged
  quality assertions with failure pixels retained before the assertion
- `professional86-key-native-v2/synthetic-review.json`: per-frame oracle errors,
  exact frozen comparison and the inspected contact image's identity
- `professional86-key-native-v3/photographed-review.json`: actual per-region
  alpha ranges and failure counts, with the inspected source/output/matte view

- `professional86-holdout-design-v1`: authored polygon over frame 500
- `professional86-key-link-v4` and `professional86-key-native-v4`: exact linked
  source identities, two passing native controls, raw failure images, assisted
  history/reopen/frozen artifacts and independent per-region review

A portable evidence archive and its per-member hashes accompany this record
under `data/professional86-key-manual-current.zip`. Failed receipts remain
separate from later results. Runtime HDR, complete photographed edge/temporal
quality and the rest of issue 86 remain pending.

## Source boundary and checks

The final changes after native execution are documentation and the fixture's
`limits` sentence reporting completed runs. Shader, package manifest, source
pixels, region boxes, mask geometry, parameters, budgets and test logic are
unchanged. The archive retains exact source-hashed link/run receipts before
that metadata wording update. Its 103 members were read back and verified:
8,519,958 bytes, SHA-256
`2e0593d45f69536e6b5a5aaecc69a0494a1b883840caa2b6a5f29dd54087fd54`.

The actual `motion-effects` package-loader/shader validation also passed:
1 passed, 0 failed, 0 ignored (0.05 seconds). No full-workspace result or
cross-platform run is claimed for this separate sample branch.

## Latest main integration

Rebased onto main `ead8ce0c` and verified commit `6f5091c0` with the
unchanged shader, preset, mask geometry and assertions. The first offline
compile stopped before test execution because the local index lacked main's
pinned harfrust 0.15.0 dependency. The subsequent locked online resolution and
link succeeded without changing Cargo.lock or any dependency declaration.
Both receipts remain separate.

The exact linked executable SHA-256 is
`75c0d0768044d9d63faf54a934b12fd09e75478be22c16f27645d72bc2ac29b3`.
Actual GPU execution: **2 passed, 0 failed, 0 ignored**, 20.20 seconds.
All 46 emitted PNGs (including raw failure, assisted results, crops,
matte and frozen outputs) are byte-exact to the previously inspected v4 images.
The actual package-loader/shader validation separately passed 1/0/0 in 0.06
seconds after this rebase. No original test assertion or threshold changed.

`data/professional86-key-main-rebase.zip` preserves the integration receipts, original dependency-cache
failure, logs, projects/commands and image identity comparison; duplicate PNGs
remain in the preceding archive. It contains 27 verified members,
392,409 bytes, SHA-256 `9a282a7d5327c575e242ec446de4b0d4c1a97cf2e93b5f9b8ee278b6ee07a7b2`. These scoped checks do not claim
full-workspace or final-head cross-platform acceptance.
