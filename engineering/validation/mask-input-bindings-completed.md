# Mask filter input-binding reuse

## Scope and coordination

This batch addresses issue #126, section III, against main `58bf6742`. Recent local branches and open PR files #344 and #348–#356 were checked before starting. Draft #344 owns sampling, renderer and compositor changes; this batch avoids those files. The Preferences work subsequently opened PR #357. Its files and the separate active custom-shortcuts branch were checked again before submission; both change desktop/UI configuration and input handling, with no overlap in `mask_gpu.rs` or its new child tests. Main was fetched again and remained at the tested base. No panel or UI presentation change is in scope.

## Baseline and intended contract

Mask filter uniforms are already pooled by pass slot. The baseline nevertheless calls `device.create_bind_group` for the two input textures on every encoded feather/combine pass. A mask whose opacity changes every frame bypasses the completed-mask fingerprint shortcut and repeats that allocation even when its scratch views are unchanged.

The regression records the actual wgpu bind-group handles used by the private filter, only under cfg(test). It does not replace or mock the GPU operation. A real 60-frame mask sequence alternates authored opacity, samples the project through the existing evaluator and mask planner, submits every mask pass and reads mask pixels. It also checks output/scratch bytes and uniform-slot capacity. A second real-GPU case independently changes equal-sized input/source views and verifies pixels and binding identity, while cloned handles, a different filter pipeline and a different output target must not require different input bindings.

## Acceptance

The two new regressions first ran against unchanged production behavior with only the cfg(test) binding observer added. Both failed specifically on reuse: **0 passed, 2 failed**, cargo exit 101. The 60-frame sequence used **180 distinct actual input bind-group handles instead of 3**. All preceding frame pixel, byte-accounting and uniform-slot assertions passed. The second test reached its final handle-identity assertions after all six pixel checks, then showed a new binding for cloned handles. Evidence: `artifacts/issue-126-mask-bindings-before.log`.

Each existing uniform slot now optionally owns its last input binding and the two view identities. Immutable layout/sampler state is shared as before. A slot rebinds only when either input view changes; pipeline, uniforms and output target are not binding resources. There is no global key map. Cache size is bounded by the existing pass-slot high-water pool. Scratch replacement or clear drops every cached binding/view first, including unused slots, so retired scratch textures are not kept alive by this cache. A boolean avoids rescanning the retained uniform pool on repeated empty clears. Budget decisions, texture allocation, uniform writes, command order, completed-mask fingerprint shortcuts and error propagation are unchanged.

The same two tests then passed: **2 passed, 0 failed**, exit zero, in 9.50 seconds. The 60-frame sequence uses only **3 distinct input bindings**, while every frame still re-encodes and has the expected opacity pixels and stable logical texture bytes. Evidence: `artifacts/issue-126-mask-bindings-after.log`.

A third regression checks dimension changes, scratch-slot shrink, an empty mask set, repeated clear and subsequent recovery. It verifies current-view identities, release of unused cached references, retained uniform capacity, pixels, output bytes and the texture ledger. All three regressions pass: **3 passed, 0 failed**, exit zero, in 9.15 seconds. Evidence: `artifacts/issue-126-mask-bindings-lifecycle.log`. The test observer is absent from production and resets per prepare operation.

Two temporary mutation controls validate that the checks detect meaningful regressions. Ignoring actual view identity yields pixel **41 instead of 82** after replacing an equal-sized source view. Disabling retirement cleanup leaves **3 cached entries instead of 1** when the scratch pool shrinks. Both selected tests fail with cargo exit 101 for those exact assertions. Evidence: `artifacts/issue-126-mask-bindings-same-view.log` and `issue-126-mask-bindings-no-reset.log`. The control script restores the original implementation bytes in a finally block; neither mutation is submitted.

**Full `cargo test -p motion-render --locked -j 1 -- --test-threads=1` passes: 210 passed, 0 failed, 1 existing ignored external-image probe, 52 test summaries, cargo exit zero.** The ignored `optional_local_original_image_probe` requires `MOTION_IMAGE_SOURCE` outside the repository. Evidence: `artifacts/issue-126-mask-bindings-complete.log`. Runs use portable MSVC, locked dependencies, one build job and one test thread, with the dedicated target `E:/Dev/aem/target/mask-input-bindings-validation`. Debug symbols and incremental builds are disabled through local environment variables to limit cache size; debug assertions remain enabled. No repository build profile is changed.

Changed production helpers and the new test module match rustfmt. The provenance guard, panel style-consumer guard and staged whitespace checks pass. No panel files, dependencies, assets, public APIs or build profiles change. The bind-group count is a real-handle regression measurement, not a frame-rate benchmark or a claim about physical VRAM residency. Local acceptance ran on Windows; other platforms remain subject to CI.

## Remaining scope

Issue #126 remains open. Vector/group buffers and opacity uniforms, the other allocation sites, first-use compilation waits and full allocation/throughput measurements require separate work. This batch must not be reported as fixing all renderer allocations or as a measured frame-rate improvement. Other PRs retain their own acceptance evidence. Cross-platform results remain subject to CI.
