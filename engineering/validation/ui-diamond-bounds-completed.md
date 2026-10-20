# UI diamond input bounds completion record

## Original scope and baseline

This batch addresses the remaining `paint::diamond` validation boundary in issue #129, section VI. The original implementation and acceptance below used main `015df56`. Before starting, the open PR files and recent branch heads were checked: #344 handles layer render sampling; #345 handles text bounds; #346 handles icon geometry and triangle clipping; #347 handles borrowed input. Some proposals share `paint.rs`, but none changes `diamond`. No work from those proposals is included here.

The current main already has the correct analytic-diamond documentation, a finite positive radius check and an alpha-empty check. Those are existing coverage, not new fixes. The missing checks are nonfinite centers and color components, and invalid derived bounds after finite-input arithmetic.

## Implementation

The helper now rejects NaN/infinity in either center coordinate or any color component before creating a command. It also validates the analytic area and its antialiasing fringe after arithmetic: endpoints and dimensions must be finite, and dimensions must be positive. This catches finite operands whose diameter or edges overflow, as well as centers/radii whose dimensions collapse under floating-point rounding.

Rejected commands do not change existing shapes, colors or creation order. A subsequent valid command still renders. Empty radius/alpha inputs retain their existing quiet empty-result behavior. Normal diamonds retain their original area, fringe, pattern, color, blending and shader. There are no panel, token, layout, hit-region, shortcut, interaction, dependency or asset changes. This is a drawing-input validation fix, not a new logging or restart mechanism.

## Baseline and targeted verification

Four new CPU regressions ran against unchanged main production code: **2 passed, 2 failed**. The failures reproduced a NaN center entering the paint list and a finite radius producing an infinite diameter. The passing controls cover the existing empty-input rules and normal analytic bounds. Evidence: `artifacts/issue-129-diamond-before.log`.

After the fix, the same four regressions pass. They exercise every center axis and color component with NaN and both infinities, radius/edge overflow, dimension collapse, retained command order, and valid recovery. Existing assertions were not modified. The targeted run also passes all four antialiasing tests, including the new GPU comparison. Evidence: `artifacts/issue-129-diamond-after.log`.

The new GPU comparison also passed before the implementation change. It requires a real wgpu adapter and compares the old helper with the current helper at 100%, 125%, 150%, 175% and 200% DPI, with zero, one and two clips. Cases include fractional centers, small and large radii, translucent and opaque colors, offscreen edges and interleaved shapes. It requires both fractional coverage and opaque interiors, so equal blank images cannot pass. All 15 cases are pixel-identical between legacy/current and baseline/final, without tolerance. The 60 captures and pixel hashes are recorded under `artifacts/issue-129-diamond/{baseline,final}/` and `gpu-pixel-comparison.json`. These are GPU-rendered test scenes, not whole-editor screenshots.

## Full acceptance

The full motion-ui suite passes: **112 passed, 0 failed, 0 ignored**, including the five new CPU/GPU regressions. Evidence: `artifacts/issue-129-diamond-ui.log`. The standalone framework example built successfully (`issue-129-diamond-framework.log`).

All four native cases (English/Chinese, default/override stylesheet) pass, including numeric drag/undo, secondary-button ownership, focus-loss cancellation, Tab guards, dock constraints and floating-window/layout checks. Override cases also verify valid reload and invalid-reload rejection. All eight main/floating input captures exactly match the prior accepted #347 captures, whose record establishes their baseline chain. Pixel hashes are in `artifacts/issue-129-diamond/native-pixel-comparison.json`; reports/captures are in `native-default/` and `native-override/`, and the runner log is `issue-129-diamond-native.log`. The adapter is NVIDIA GeForce RTX 4070 Laptop GPU, Vulkan. These are standalone framework windows with injected normalized input, not physical-device input or certification of every editor panel.

The complete motion-desktop suite passes: **593 passed, 0 failed, 1 existing native-audio test ignored**, in 210.03 seconds. The process exited successfully; no existing assertion or ignore was changed. Evidence: `artifacts/issue-129-diamond-desktop.log`. Combined acceptance is **705 passed, 0 failed, 1 existing ignore**.

Provenance passed for 1454 tracked paths. The panel style guard, new-test/changed-block formatting and staged whitespace checks pass. The panels diff is empty. Main remained `015df56` at the final baseline check.

All Cargo runs use the portable MSVC environment, `-j 1`, `--locked` and the dedicated target `E:/Dev/aem/target/observability-diamond-validation`, avoiding artifacts from other worktrees. Reproduction commands:

```text
cargo test -j 1 -p motion-ui --locked -- --test-threads=1
cargo build -j 1 -p motion-ui --example framework --locked
cargo test -j 1 -p motion-desktop --locked -- --test-threads=1
python tools/ci_ui_framework.py --binary E:/Dev/aem/artifacts/issue-129-diamond/bin/framework.exe --output E:/Dev/aem/artifacts/issue-129-diamond/native-default
python tools/ci_ui_framework.py --binary E:/Dev/aem/artifacts/issue-129-diamond/bin/framework.exe --output E:/Dev/aem/artifacts/issue-129-diamond/native-override --style desktop/ui/styles/example.override.json
python tools/validate_provenance.py
```

## DESIGN.md disposition

All twelve section 0.7 baseline rows are **retained with reason**: this patch changes no panel code or presentation. Path row steps, two-column sizing, control heights, numeric backgrounds and boolean presentation remain outside this batch. Effect row steps, boolean/enum controls, hover styling and hierarchy remain outside this batch. Timeline gutter coordinates and snap-guide styling are preserved. No compliance claim is made for those rows. No context menu changes.

## Remaining limits

Issue #129 remains open. This batch does not complete configurable stroke joins, grapheme editing/shaping, or all remaining geometry and input allocations. Prior atlas, ownership and dock fixes and the pending text/icon/input PRs retain their own evidence and scope. The helper cannot validate arbitrary `Shape::Procedural` values constructed directly by other callers; this change covers its public `diamond` entry point.

## CI refresh against main `34d1d7a`

The old push and PR runs completed with cancellation after macOS Intel reached the 60-minute job budget. Main now includes #364's 90-minute Intel budget. The existing PR branch was rebased onto that actual main revision without conflicts; its diamond implementation and all existing assertions are unchanged. This refresh does not skip tests, alter pixel tolerances or rerun an identical configuration in search of a green result.

Before refreshing, the complete file lists of 14 open PRs, 18 recent branches and every dirty worktree were reviewed. Only this PR owns the diamond change. An existing dirty worktree shares paint/capture files for a historical wgpu API migration; its edited pipeline/capture code does not touch the diamond helper or the added regression. Its files remain untouched.

Fresh Windows acceptance, with the portable MSVC environment and inactive reusable target `target/image-proxy-cache-shards-validation`, `--locked -j 1`, local test/dev debug information and incremental compilation disabled:

- Complete `motion-ui` suite: **129 passed, zero failed, zero ignored**, across 19 result summaries including the compile-fail doctest
- The real GPU comparison again passes all 15 DPI/clipping cases. Each refreshed output also matches the previously accepted output's decoded RGBA bytes exactly; the comparison checks dimensions and all pixels, with no tolerance
- The standalone framework builds, and all four native English/Chinese default/override cases pass through `ci_ui_framework.py`, including numeric undo, native floating windows, dock and style checks. Two refreshed main-window captures were visually inspected
- `cargo check -p motion-desktop --locked -j 1`: passed, with 21 existing warnings

Fresh logs: `artifacts/issue-348-ci-refresh-{ui,framework,native,desktop-check}.log`. GPU captures, RGBA hashes, native reports and captures: `artifacts/issue-129-diamond/ci-refresh/`. These refreshed results do not relabel the historical 593-test desktop run above as a run on the new base; current full desktop and cross-platform results remain subject to CI on the updated head.

The scope still contains no panels, menus, tokens, UI interactions or layout changes. The original DESIGN disposition remains applicable. No aggregate issue is closed by refreshing the validation budget.


## Native CI integration refresh — 2026-10-20

To clear the existing PR queue, this branch now includes the reviewed #368 head
`1381cc1`, including the already merged #364 Intel budget. The PR's Rust
implementation and regression files are byte-identical to the previous head;
the incoming CI/test infrastructure changes are tracked in
`native-ci-failure-evidence.md`. Main's zlib source attribution update is retained
where applicable. No existing assertion, timeout or ignore policy is relaxed.

This is a real baseline change, not a rerun of an unchanged failed head. The
sampling fixture avoids crossing its original 100-second loop during a slow
run while retaining strict adjacent-frame progress checks. PNG diagnostics
publish in-flight, queue and GPU context on the original timeout; adding that
evidence is not described as fixing the underlying PNG timeout. Evidence paths
are absolute and fall inside the always-upload directory.

The prior per-PR validation recorded above remains historical evidence. A local
combined-tree acceptance run is in progress in `artifacts/pr-queue-complete-tests.log`;
it is not claimed as a full successful run or a replacement for this head's CI.
The local combined timeline-shortcut suite passed all 39 tests and the tools
suite passed 54 of 55 checks with one existing optional external-loader skip.
Those combined results include other PRs, not just this individual branch.

The branch is prepared for stacking on #368, then returning to main after the
parent merges. Actual publication and final remote checks must be verified
separately. No new optimization PR, dependency, asset or visual change is added.
