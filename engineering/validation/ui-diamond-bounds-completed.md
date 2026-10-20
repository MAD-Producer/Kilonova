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

## Native Windows reply-wait follow-up

The refreshed current head failed Windows CI in run `39370955457`, job
`118458782325`: 610 desktop tests passed, two failed and one existing test was
ignored. Both failures used the unchanged 10-second Present reply deadline.
The PNG failure snapshot identified Present in flight, an empty queue, and
the DX12 Microsoft Basic Render Driver. Artifact `12243674330` was downloaded
and its ZIP CRC checked. Raw logs and the original timeout snapshot are retained
under `artifacts/pr-queue/`; this is distinct from the earlier playback-loop
fixture defect and is not attributed to diamond bounds checking.

Both hidden-window fixtures create a window on their test thread and then
block that thread waiting for GPU-worker replies. They never pump the native
window queue. [Microsoft's DXGI guidance](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/dxgi-best-practices#multithreading-and-dxgi)
documents that cross-thread Present can send synchronous window messages and
wait for the owner thread. This is a concrete fixture defect; the archived
remote evidence does not identify the exact worker call or prove this is its
only cause.

A Windows-only test helper pumps the hidden window's native event loop while
polling the existing reply, using the original absolute deadline. It forwards
success, timeout and disconnect without retrying engine commands. Only the two
native fixtures use it for attach and Present waits. Production event handling,
GPU resources, draw output and shutdown are unchanged; no timeout, progress
assertion or pixel tolerance is increased. The sampling report now explicitly
includes test-only native message pumping in its measured scope.

The controlled regression sends harmless WM_NULL synchronously from a worker.
A plain channel wait times out while that send is pending; the helper services
the message and receives the original reply. It also checks a held sender's
timeout, receiver disconnection and a ready reply with zero wait. Compiling the
actual helper through an isolated native driver passed this test (0.61s).
A separate copied-helper mutant replacing message pumping with sleep failed
at the positive reply assertion (exit 101); production source was not mutated.
The initial standalone link omitted the prebuilt Windows import-library path
and failed; the corrected command linked and ran. Its failed attempt is not
counted as a test execution. Cargo integration and the actual native fixtures
remain pending at this checkpoint, as does updated remote Windows CI.

The actual Cargo desktop binary also passed the new controlled-message test
(1 passed, 613 filtered, 0.23s), after a default unoptimized debug-info build
completed in 10m45s. This is separate from the fast standalone helper run.
Evidence: `artifacts/pr-queue/348-native-message-control.log`.

After cleanly integrating the already merged grapheme baseline and #374's CI
routing, all three targeted Cargo tests passed on the actual final source:
controlled native messages (0.27s), bilingual PNG shell (19.02s), and four-mode
sampling (40.20s). Every sampling mode retained five warmup and sixty strictly
advancing recorded frames. All twelve retained PNGs decoded successfully.
Logs: `348-final-message-control.log`, `348-pumped-png.log` and
`348-pumped-sampling.log` in `artifacts/pr-queue/`. Native files and full cost
records are in `348-pumped-native/` there. Provenance (1,536 paths), panel style
consumers, whitespace, and all seven workflow checks passed.

These local windows use the normal Vulkan-first desktop backend sequence.
Setting WGPU_BACKEND=dx12 did not override that native surface sequence; no
local software-DX12 or WARP acceptance is claimed. The unchanged remote Windows
job must verify that backend with this new head. Existing local Epic overlay
loader errors remain in Error stderr and file logs; they are not suppressed.
No full desktop suite was rerun for this fixture-only change; the earlier
combined 1,196-case run and Release GUI acceptance remain separately scoped.
The PR temporarily depends on #374 to avoid duplicating validation on its next
push. Its original diamond implementation and regressions remain unchanged.
