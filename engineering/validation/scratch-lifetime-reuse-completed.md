# Scratch lifetime and size reuse — desktop acceptance

Issue: [#67](https://github.com/MAD-Producer/Kilonova/issues/67)

The maintained-desktop implementation and local acceptance are complete on
source `2f243de3176ed4714ee9dae17f20f3b0acefdf80`, integrated with main
`1d1f284245fcd50391f672ef79653d09d432e219`. PR CI and merge remain separate
delivery gates. Documentation and evidence packaging follow the measured code
commit; the compiled inputs remain unchanged.

The [protocol](scratch-lifetime-protocol.md) describes logical values, physical
allocations, clear boundaries, execution order and packed-plan v10. The
[acceptance manifest](scratch-lifetime-reuse-acceptance.json) identifies the
actual gates and immutable [raw evidence](data/scratch-lifetimes-2f243de3.zip).

## Requirement disposition

| Issue requirement | Implementation and acceptance evidence | Disposition |
|---|---|---|
| Measure mixed-size waste; allocate by size, format, usage and lifetime | Inclusive def/use graph and deterministic byte-weighted interval placement; eight paired native captures below | Complete for maintained desktop |
| Preserve linear/sRGB, premultiplied/straight and float accumulation semantics | Immutable logical classes; native formats stay distinct; different semantic values share storage only after their last consumers and a full attachment clear; packed uses/uniforms and native pixels independently compared | Complete |
| Update planner, GPU pool, UV mapping and binary consumers together | Dynamic physical pool, draw-specific output IDs, physical backing UV fractions, v10 descriptors, Host caller-buffer metadata, production Kotlin compilation and five real JVM parser tests | Complete; Android device rendering excluded |
| Alias only after the final consumer; retain padding/output bounds | Inclusive pass and terminal consumers, atomic draw/replay units, independent dependency snapshots, full-padding clears, poisoned-padding readback and more-than-eight native allocations/reorder/recovery | Complete |
| Large → small → large, long blur, padding, combined generator/matte/nested | Source-identical authored fixtures and controls on the same adapter; raw RGBA and decoded PNG comparisons | Complete |
| Planned bytes equal actual allocations; no pixel contamination; measure the 40% hypothesis | Every active top-pool descriptor matches native dimensions, format and ledger bytes; exact pixels in all eight captures; reductions range from 0% to 87.5% | Complete for measured fixtures; no universal 40% claim |

Android physical-device and GLES rendering acceptance was excluded by the
user's maintained-desktop scope. The GLES consumer was updated and its actual
production Kotlin compilation and JVM protocol tests passed. Those checks do
not constitute a mobile rendering result.

## Paired native results

Baseline is production main `1d1f2842` plus the recorded fixture driver;
candidate is `2f243de3`. Both use the same driver SHA, authored project hashes,
frame 17, 512 × 512 output and adapter: NVIDIA GeForce RTX 4070 Laptop GPU,
Vulkan, NVIDIA 591.86. Their executable hashes differ and their actual packed
versions are 9 and 10 respectively.

| Capture | Baseline scratch bytes | Candidate scratch bytes | Exact pixels |
|---|---:|---:|---|
| Mixed aspects | 1,048,576 | 131,072 | Yes |
| Large before | 1,572,864 | 1,572,864 | Yes |
| Small | 4,096 | 4,096 | Yes |
| Large after | 1,572,864 | 1,572,864 | Yes |
| Padding and long blur | 704,952 | 611,928 | Yes |
| Effected matte | 716,800 | 716,800 | Yes |
| Generator, matte and nested | 5,242,880 | 5,242,880 | Yes |
| Mixed aspects revisited | 1,048,576 | 131,072 | Yes |

The mixed-aspect fixture saves 87.5% logical scratch capacity; the padded blur
fixture saves approximately 13.2%. Other fixtures show no capacity reduction.
These Debug captures establish pixel/capacity correctness, not lower frame
time, bandwidth or physical driver VRAM consumption.

The combined scene includes an independently captured generator-disabled
control. Enabling the generator changes 2,578 pixels; the nested/matte control
has 4,725 visible colored pixels, of which 4,685 remain unchanged with the
generator enabled. Both contributions are visible in the same frame. The
control and combined pixels also match exactly across baseline and candidate.
Archived first-party SDK generator fixtures are validation inputs; removed
particle packages are not restored to the product registry.

## Actual gates

The complete integrated command was:

```text
cargo test -p motion-render -p motion-host -p motion-desktop --features motion-host/diagnostics --offline --no-fail-fast -- --test-threads=1
```

It finished with 113 suite summaries: **1,693 passed, 0 failed, 2 ignored**.
These are executions on the identified source, not a permanent capability
count. The two existing ignores require an actual native audio output device
and external `MOTION_IMAGE_SOURCE`. No assertion or ignore was changed to make
this gate pass.

Additional accepted gates:

- `cargo check -p motion-desktop --offline`
- `python tools/build_desktop.py --task check` with an explicit E-drive target;
  its actual command uses `cargo check --locked --release`
- Current production Kotlin compilation and five JVM parser tests, with no
  failures, errors or skipped JVM cases
- Nine focused phase-adjustment tests, two new actual GPU scratch-reuse tests
  and the actual Host caller-owned serialization-buffer test
- Eight paired native captures and independent raw/decoded-pixel,
  descriptor/ledger, original-use/uniform, inclusive-lifetime and UV checks
- Extraction and successful replay of both independent verifiers from the
  finished evidence archive

The focused Rust tests overlap the complete gate and are not added to its
count. At supported maximum layer/effect counts, the planning regression
generates 6,272 passes, three allocations and 12,288 bytes, with deterministic
packed output and no same-pass input/output aliases. No timing threshold is
substituted for correctness.

## Failure history and limits

The archive retains failed and invalid attempts with their original source,
logs and terminal results. An early fixture failed to compile because of a
`Result` alias; an early candidate reused the baseline executable and is
explicitly invalid; the original 96 MiB tint boundary exposed a host-utility
classification error; an intermediate padding-clear build omitted
`multiview_mask`. Later acceptance does not relabel any of those attempts.

The complete pre-integration Render gate ended 570 passed / 1 failed / 1
ignored. Its failure, `effect draw 2 has no scratch output`, came from using a
phase-geometry index as a center-time effect-draw index. The implementation now
passes those indices separately. The unchanged failing assertion passes in
the focused nine-case suite and the complete integrated regression.

Allocation replacement remains transactional: later refusal leaves the
previous pool, identities, bindings and admission intact. Healthy empty plans
retain the existing warm-pool policy for up to 120 idle frames. Zero active
descriptors therefore do not mean retained native storage has been freed;
the live ledger and admission still count it.

No panel, layout, style, interaction, dependency or third-party asset changed.
Project-selectable precision and additional semantic formats belong to #72;
effect fusion and typed Render DAG work remain #73 and #78. This issue does
not claim those subsequent capabilities.
