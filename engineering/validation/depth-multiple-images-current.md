# Multiple current image-depth results

## Accepted implementation and evidence

Implementation `d18aadd77d2108b87801348f61cc4d9c9db3931c` includes main `8c271bd361b0ecdd4d57e29d3bfe2d7ecaff5f4f` and the native image-depth
implementation published in PR #539. This followup retains independently admitted
results for multiple image assets and routes each calibrated layer to its own
actual prepared RGB input and immutable depth resource.

The source-bound integration gate passes **165 host tests**
across the library and nine integration targets, with zero failures or ignored
tests. Provenance, panel-style, depth-disabled host compilation and desktop
FFmpeg/AI-depth compilation also pass. Each step verifies the captured source
hashes. Focused evidence additionally records three retained-pool tests, fourteen
session tests and thirteen video-contract tests; overlapping focused/full cases
are not added together as distinct coverage.

## Behavior and boundaries

- A real GPU/PNG test renders two separately inferred pictures around a physical
  blue depth plane. Near green and near red regions independently occlude the
  other picture and the plane. Reversing inference order preserves pixels.
- Starting a replacement request removes the previous admission for that asset,
  including a changed model or crop, while keeping unrelated admitted assets.
  A controlled blocked backend proves the scene stays pending and cannot
  overwrite the previous successful PNG until the new result is admitted.
- Explicit cancellation and frame changes clear ownership. Reacquiring both
  sources restores the same actual PNG pixels. No completed result from another
  frame, document, revision, source or processing identity is substituted.
- Retained scalar, validity and confidence planes together are limited to
  192 MiB and `MAX_LAYERS` samples. This is a retained-result pool limit, not a
  total process-memory claim; worker inputs, mesh staging and renderer allocations
  have their own limits. Refusal preserves other entries and reports a typed
  error with request/asset IDs and required/allowed capacity.
- `depth_status` exposes retained sample count, retained bytes and a saturating
  lifetime admission-rejection counter. Clearing retained ownership does not
  reset that lifetime counter. Tests cover capacity refusal, reference release,
  count limits, saturation and successful recovery.
- Existing source-change, model-change, canceled-ticket and video PTS tests run
  unchanged. This increment does not enable video geometry, static-image reuse
  across playback frames, nested/effect/matte/exposure depth or DOF.

The actual PNG pixel assertions execute in temporary directories; those PNGs are
not retained in this archive. The archive contains the test sources, original
command output and source identities. No visual inspection is claimed here.
There are no panel, style, dependency or external-asset changes in this followup.
Android is retired and is excluded from acceptance. Final whole-workspace and
maintained-desktop cross-platform #87 acceptance remain open.

## Archive

[`data/depth-multiple-images-native-v1.zip`](data/depth-multiple-images-native-v1.zip)

- SHA-256: `217a82e19524445f93afdc7de9dfbb29b70f552e50480ba7322213630c1c6a4d`
- Entries: 24; each entry was read back and compared with its original
- Parent native image-depth evidence: `depth-surface-native-current.md`
