# Plane alpha coverage in native shared scene depth

## Implemented behavior

The accepted opaque-image checkpoint is extended with translucent solid/image
planes and layer opacity. Compatible meshes and planes use one camera and
fragment depth. Opaque geometry writes depth first; alpha geometry tests that
depth without writing it, and joins the existing global mesh primitive order.
Linear premultiplied color is blended once. A zero-alpha image texel preserves
visible rear geometry. Equal-depth opaque plane/mesh ties preserve the earlier
mesh winner. Pure-plane compositions retain their original BSP path.

The source revision ownership, admission and immutable vertex contracts from
the opaque-image increment apply to alpha images too. Solid vertices still cost
192 bytes per plane, images 240 bytes. The bounded pipeline cache now has at
most 32 keys: four formats, two sample counts, two source kinds and two alpha
modes. Shader variants are compiled only on cold keys and keep the existing
Info start/default diagnostics. No healthy per-frame file IO is introduced.

## Actual controls and acceptance

The first test draft fails compilation on an unspecified bytemuck output type;
an explicit f32-to-u8 type resolves it without changing expected behavior.
The unchanged parent then fails all three actual GPU regressions: solid alpha
and image alpha cover a nearer mesh, and alpha planes bypass mesh global order.
The first native implementation passes both occlusion tests. Its idealized
Float32 endpoint assertion fails because the actual GPU sRGB material sample
differs from one by an ULP. The independent reference is corrected to render
each isolated source and calculate linear source-over on CPU. Byte equality
remains exact, with no new or relaxed tolerance. The exact corrected tests run
again on the unchanged parent in an isolated control tree: all three still
fail for their intended causes. They then all pass on the new implementation.
All original test histories are retained.

Final source-bound native acceptance has **421 passed, 0 failed, 0 ignored**
across 13 targets, plus desktop FFmpeg/AI-depth compilation, provenance and
panel-style checks. Eight new tests exercise real saved/reopened assets with
removed source files, both authored orders, all six permutations of three
alpha sources, an independent CPU source-over oracle in Float32, solid/image
coverage in four precisions at 1x/4x, actual PNG holes with nearer/rear geometry,
layer opacity, reverse queued source revisions and abandonment, opaque coplanar
ties, exact one-byte-short admission, invalid opacity refusal, unchanged output
after failure and successful recovery. Existing renderer, model, scene, mask,
matte, effect, composition, exposure and nested tests remain unchanged.
Overlapping previous checkpoint counts are not added to this total.

## Remaining scope

Transparent primitives still use their camera-forward center for global order.
This is not exact intersecting translucent-fragment ordering or OIT. Mixed
intersecting alpha geometry and the preservation/extension of BSP semantics
remain explicit work, rather than a completed transparency claim. Effects,
masks, mattes, nested compositions, nonnormal blends and incompatible sampling
retain their existing receiver boundaries. Imported-camera project selection
and tracks, persisted current-frame calibration/reprojection and DOF remain
open with #74/#87. This is not full-workspace, GUI or cross-platform acceptance.
The older opaque checkpoint's alpha-fallback description is historical and is
superseded by this increment; its source-bound evidence remains unchanged.

No UI/panel/style edits, dependencies or third-party assets are added. Android
is retired and excluded. Pre-existing assertions, tolerances, budgets and
timeouts are unchanged. Neither #74 nor #87 is closed by this checkpoint.

## Source alignment and evidence

PR528 merged at `21093ee76231b76bbeb53055689cae4858c51897` after required CI passed, with its actual Chinese
squash title verified. Its tree is identical to the original published head.
The opaque-image commit is reparented to that main without any tree change.
After this acceptance, the alpha increment is reparented onto that identical
opaque-image tree; source/test hashes and the complete final tree are checked.

[Source-bound failures, independent control and native acceptance](data/transparent-plane-depth-native-v1.zip):
SHA-256 `c0bbf08a1840a306ae6b5d9ff346caaeddd0bb2fd8435b016826deaf8378c460`, 901116 bytes, 60 entries.
Every entry is read back and hash-verified. Only changed source is copied;
full source hashes identify the baseline. Executables and shared libraries are
excluded. Reparenting receipts retain old/new heads without claiming a rerun.
