# Owned original image and calibrated depth

Issue: #87. Integration base: `895c15c815875602be59f69ab7ee7fe25ec3eb58` (PR #541).
The archive binds the exact changed source hashes before the implementation commit.

## Behavior and evidence

The depth worker retains the full original straight sRGB8 RGBA allocation used
to prepare inference RGB. Color and admitted depth now share immutable ownership;
rendering a GPU cache miss cannot silently decode newly changed source pixels.
Crop, rotation and mirroring still affect inference input only. Rendering uses
the original full image and existing surface UVs, without inventing a crop fill.

The controlled native regression first failed against the old implementation:
changing the source file after admission changed color while geometry stayed old.
After the fix, independent fresh headless PNG renderers preserve the exact
original pixels through file replacement and deletion, without rerunning inference.
Explicit cancellation still rejects capture, and pending/obsolete worker tickets
retain their original frame/revision/document rejection contract.

The native renderer additionally verifies original-image admission after GPU
eviction, full/preview/full quality changes, and no repeat upload on healthy
preparation. Ordinary unowned sources still require valid source paths. Existing
calibrated geometry tests retain their four-precision, camera, parent, resource
replacement and pending-command coverage; the new host fixture is Unorm8.

## Ownership and limits

- A full original image is limited to 4096 by 4096 RGBA8 and 64 MiB capacity
- The allocation moves into shared ownership without a second full-size copy
- A full RGBA content hash is computed once during worker source preparation
- GPU cache tokens retain only that fixed hash, never original pixel ownership
- Scalar planes plus original image allocation capacities share the session's 192 MiB result allowance
- Status reports scalar bytes and original image bytes separately
- Refusal preserves other sources; replacement and clearing release the old admission
- Preview resizing uses the existing linear-alpha box filter and checks output plus row-accumulator budget
- A fresh semantic sample drops old image cache ownership until independently readmitted

The allowance is per session result owner, not a process-wide memory claim.
Transient worker RGB/codec allocations, GPU textures and other output owners
remain separate budgets. Frozen snapshots still persist scalar ownership and
verify copied source files; this change does not add raw RGBA to durable payloads.

## Validation

- Host library, depth session and video targets: 137 passed
- Owned image unit: 1 passed
- Complete native calibrated geometry target: 7 passed
- AI-disabled host compilation, provenance and panel style checks passed
- Zero failed/ignored tests and unchanged source hashes after every command
- Original failure, raw logs, commands, changed source and native PNGs retained

No UI, panels, styles, external assets, dependencies or existing assertions change.
This is a required source-ownership step toward live playback reuse, not a claim
that timeless reuse, calibrated video, complex effects/depth, transparency, DOF,
full workspace acceptance or remote cross-platform acceptance are complete.
Android is retired and excluded.

Archive: [owned-depth-image-native-v1.zip](data/owned-depth-image-native-v1.zip).
SHA-256: `36bf512ff2eaea4114f45a945db755ef350f1dffc039b3b7d3c93dcabfc0982b`. Every entry was read back and byte-compared.
