# Presentation output and rejection-test audit

## Scope and coordination

This batch addresses issue #135, section V, against main `58bf6742`. Before starting, recent local branches and the file lists of open PRs #344 and #348–#355 were checked. No open PR changes `crates/motion-render/tests/presentation.rs` or this document. The sampling work in draft #344 owns production renderer/compositor code; this batch changes tests only and does not modify that implementation.

## Existing coverage and remaining gap

The historical claim that presentation tests observe no GPU failures is no longer true at this baseline. The shader test checks the renderer's health after device polling, and `presentation_validation_failures_are_visible_to_the_test` deliberately uses incompatible target formats and asserts the resulting validation failure. The zoom/pan/fit test also has actual pixel assertions. These are existing observations and assertions, not new fixes.

However, the standalone shader test still cannot distinguish correctly rendered pixels from a valid shader that renders the wrong image. This batch strengthens that existing test with four independent expected quadrant colors, including transparent and half-alpha pixels, across RGBA/BGRA and sRGB/UNORM output formats. Raw surface readback avoids the renderer's straight-alpha PNG conversion. A per-channel tolerance of three byte values matches the existing GPU pixel-test convention. GPU validation/health checks remain in place.

The other section V example, `resource_rejections.rs`, already checks typed budget/dimension refusals, scope, required bytes, serialized budget, recovery/cache clearing and unrelated invalid errors. Its display-text comparison additionally preserves the legacy public message. That comparison alone would be insufficient, but it is not the only assertion now present. This is recorded as existing coverage; the tests and production code are retained without redundant changes.

## Acceptance

The unchanged baseline shader test was first run with a temporary, valid fragment shader that always outputs opaque black. It still passed: **1 passed, 0 failed**, exit zero, in 5.38 seconds. This demonstrates the missing output signal; it is not a product-rendering defect. Evidence: `artifacts/issue-135-render-black-baseline.log`.

With the strengthened test and the original production shader, the complete presentation binary passes: **3 passed, 0 failed, 0 ignored**, exit zero, in 5.78 seconds. This includes the unchanged navigation pixel test and deliberate incompatible-format validation test. Evidence: `artifacts/issue-135-render-presentation-after.log`.

Four controlled, temporary shader mutations were then run against the same pixel assertions. Each shader is valid and each run fails specifically on expected-vs-actual surface bytes, with cargo exit 101:

| Mutation | Observed Rgba8Unorm pixel | Expected | Evidence log under `artifacts/` |
| --- | --- | --- | --- |
| Constant opaque black | `(16,16)` = `[0,0,0,255]` | `[102,153,204,255]` | `issue-135-render-black-after.log` |
| Omit linear-to-sRGB encoding | `(16,16)` = `[34,81,154,255]` | `[102,153,204,255]` | `issue-135-render-unencoded-after.log` |
| Flip vertical UV | `(16,16)` = `[188,0,0,127]` | `[102,153,204,255]` | `issue-135-render-flipped-after.log` |
| Force output alpha to one | `(16,48)` = `[188,0,0,255]` | `[188,0,0,128]` | `issue-135-render-opaque-after.log` |

The mutation runner restores the exact original shader bytes in a finally block. After completion, `git diff --exit-code -- crates/motion-render/src/present.wgsl` passes. No mutation is part of the submitted change. Assertions were not weakened to make any control pass.

The complete `cargo test -p motion-render --locked -j 1 -- --test-threads=1` suite then completed with **207 passed, 0 failed, 1 existing ignored local-original-image probe**, across 52 result summaries including doc tests, with exit zero. The ignored test explicitly requires external `MOTION_IMAGE_SOURCE`; no tests or assertions were disabled for this batch. The strengthened presentation test and all three unchanged resource-rejection tests also pass in this full run. Evidence: `artifacts/issue-135-render-complete.log`.

The dedicated build target is `E:/Dev/aem/target/render-output-contract-validation`. Runs use portable MSVC, locked dependencies, one build job and one test thread. Local dev/test debug information and incremental compilation are disabled through environment variables to limit rebuildable disk usage; debug assertions remain enabled and repository profile configuration is unchanged.

Changed Rust sections pass rustfmt verification. Panel style and provenance guards and staged whitespace checks pass. Production renderer/shader and panel diffs are empty. Main was fetched again before submission and remains `58bf6742`; no integration merge is required. The new local Preferences branch changes dialogs, workspace and style configuration, with no overlap in this batch's files.

## Limits

Issue #135 remains open for its other sections. This test exercises the shared presentation shader on the local desktop GPU; it is not an Android-device or Android lifecycle acceptance claim. No product behavior, panel, style, dependency, asset, error handling or GPU-skip policy is changed. Cross-platform checks remain subject to CI.
