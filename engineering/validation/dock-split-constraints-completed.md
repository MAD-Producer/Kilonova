# Dock splitter constraints — issue #129 increment

Current integration: 2026-10-19, base main `452f659d6c92ebfe60554b21bd26f45e3829f5da`, branch `fix/dock-split-boundaries`.

The original implementation is preserved on `fix/dock-split-constraints`, commit `c358fb8f9e44b699947d0ba6f9f5ef8be324e00b`, based on `a801e54f155782647f69a361b71f32e560c10bf0`. Verification under the historical headings below belongs to that baseline. Current integration evidence is recorded separately at the end.

## Scope and original collision review

This increment fixes the split-constraint and rejected-split items in
[issue #129](https://github.com/MAD-Producer/Kilonova/issues/129). It does not
close the UI framework umbrella issue or include the separate input-ownership
work on `fix/ui-input-ownership` (`cf14bbef`).

The pre-edit audit at `2026-10-18T09:15:57Z` inspected 150 worktrees, recent
branches and open PRs #327, #329, #330, #331 and #332. The record is
`E:/Dev/aem/.tools/p2-dock-split-branch-audit.json`. The older dirty
`desktop-basics-nodes-integration-20261010` tree contains historical splitter
collection/default-layout edits, not this constraint fix; it was read only.
Other worktrees and the root checkout were preserved. This branch starts from
the fetched main and changes no panel code. Documentation and native-probe
additions use separate insertion points and a separate dock report to avoid
overlapping the active temporary-maximize PR and input-ownership report edits.
Read-only `git merge-tree --write-tree --name-only` checks against
`feat/temporary-panel-maximize`, `feat/node-canvas-visibility` and
`fix/ui-input-ownership` all reported clean merges. No branch was actually
merged; this establishes textual compatibility, not combined runtime acceptance.

## Behavior and compatibility

- Workspace dragging and the public helper now share `dock::clamp_split`.
  The existing `MIN_DOCK` and `MIN_TIMELINE` logical-pixel minima apply without
  an additional 5% floor. For example, a horizontal child in a 4000-wide parent
  can reach 160 rather than 200 logical pixels. The existing 45% fallback for
  parents too small to fit both minima is retained.
- Rejected `Node::split` calls return false without changing the existing tree.
- Nested hit geometry and solved placement geometry share `dock::divide`.
  A below-5% saved split no longer has a wider invisible nested hit region.
  Existing hit tolerances are unchanged.
- Stored ratios must be finite and strictly between zero and one. This admits
  the new below-5% pixel-minimum layouts while rejecting collapsed or reversed
  splits. The JSON schema and field names are unchanged. Old binaries with the
  former 5% validation can reset such layouts to their default; backwards
  reading by an old binary is not claimed.
- Invalid helper proposals or nonpositive/nonfinite extents use an even split.
  No project operation, history model, style token, panel row or default preset
  ratio changes. This is a functional docking fix, not an observability patch.

`DESIGN.md` section 4 and the framework README document the shared policy.

## Original regression evidence (historical baseline)

The five new `desktop/ui/tests/dock_constraints.rs` tests were first run against
the unchanged library: **1 passed, 4 failed**. The failures reproduced rejected
operation mutation, percentage-based minima, rejection of valid narrow saved
layouts and propagation of nonfinite proposals. All five pass with the fix;
existing assertions were not weakened.

The parameterized drag test covers all four edges, both ends, an undersized
parent, default/minimum-sized extents and a 4000-wide parent. It verifies that
actual workspace interaction agrees with the public helper. The nested test
also rejects a click beyond the painted child before resizing its real handle.

Verified locally:

- Full `motion-ui` suite: **87 passed, 0 failed, 0 ignored**, including GPU tests
- `cargo check -p motion-desktop`: passed, existing warnings retained
- Full serial `motion-desktop` suite: **423 passed, 0 failed, 1 existing native-audio test ignored**, 391.14 seconds
- Framework example build: passed
- Python tool suite: **54 run, 53 passed, 1 existing skip**, 47.374 seconds
- Clean-room provenance guard, panel style-consumer guard and `git diff --check`: passed

Rust commands used the portable MSVC environment, `CARGO_BUILD_JOBS=2`,
`--offline --locked --target-dir E:/Dev/aem/target`. Tests were not skipped to
get a passing result.

```text
cargo test -p motion-ui --test dock_constraints --offline --locked --target-dir E:/Dev/aem/target
cargo test -p motion-ui --offline --locked --target-dir E:/Dev/aem/target
cargo build -p motion-ui --example framework --offline --locked --target-dir E:/Dev/aem/target
cargo check -p motion-desktop --offline --locked --target-dir E:/Dev/aem/target
cargo test -p motion-desktop --offline --locked --target-dir E:/Dev/aem/target -- --test-threads=1
python -m unittest discover -s tools -p 'test_*.py'
python tools/validate_provenance.py
```

Logs are under `E:/Dev/aem/.tools/p2-dock-`: `before.log`, `after.log`,
`ui-tests.log`, `framework-build.log`, `desktop-check.log`, `desktop-tests.log`,
`python.log` and `native-{default,minimum,wide}.log`.

## Original native window evidence (historical baseline)

All six cases passed on Windows with Vulkan, RTX 4070 Laptop GPU, scale factor
1.0. These are actual native component-gallery windows and GPU captures, not
editor project screenshots or mockups. Normalized press/move/release input goes
through the native redraw path and workspace interaction; surface presentation
is checked before capturing the minimum-width state.

| Case | Actual logical size | Locales | Observed minimum width |
| --- | --- | --- | --- |
| default | 1600 x 1000 | zh, en | 160 |
| minimum | 900 x 600 | zh, en | 160 |
| wide | 4000 x 1200 | zh, en | 160 |

Each case checks rejected splitting, a real resized-layout file round-trip,
restoration of the original gallery workspace, unchanged model/undo history,
unchanged native size, and existing numeric undo/floating-window smoke checks.
The native runner requires the added report flags and artifacts. The gallery
accepts `--size WIDTHxHEIGHT`; its normal default remains 1280 x 800.

```text
python tools/ci_ui_framework.py --binary E:/Dev/aem/target/debug/examples/framework.exe --output E:/Dev/aem/artifacts/dock-split-constraints/default --size 1600x1000
python tools/ci_ui_framework.py --binary E:/Dev/aem/target/debug/examples/framework.exe --output E:/Dev/aem/artifacts/dock-split-constraints/minimum --size 900x600
python tools/ci_ui_framework.py --binary E:/Dev/aem/target/debug/examples/framework.exe --output E:/Dev/aem/artifacts/dock-split-constraints/wide --size 4000x1200
```

For each case and locale, artifacts include `framework-{locale}.json`, `.dock.json`,
`.dock-before.png`, `.dock-minimum.png`, `.dock-layout.json`, `.png`,
`.floating.png` and the original gallery `.layout.json`. The before/minimum
images compare states of the same gesture on the fixed build, not before/after
source revisions. Minimum-window Chinese captures were visually inspected and
shown to the user.

## DESIGN.md section 0.7 disposition

All listed rows are retained because panel painting and control geometry are
outside this dock-constraint fix; this is not a claim that baseline defects are
resolved.

| Baseline item | Disposition / reason |
| --- | --- |
| Path literal vertical steps | Retained — no path rows changed |
| Path two-column split | Retained — no property-column sizing changed |
| Path mixed button heights | Retained — no control sizing changed |
| Path custom numeric background | Retained — no numeric painting changed |
| Path boolean button | Retained — no boolean presentation changed |
| Effect row heights | Retained — no effect rows changed |
| Effect boolean text buttons | Retained — no effect controls changed |
| Effect enum full-width button | Retained — no enum controls changed |
| Effect manual hover alpha | Retained — no color changes |
| Effect indentation | Retained — no property hierarchy changes |
| Timeline gutter coordinates | Retained — no timeline column changes |
| Timeline snap-guide color | Retained — no timeline guides changed |

## Remaining limits

Other #129 items, including atlas lifetime, text/grapheme behavior and allocation
work, are not certified by these tests. `tabs::draw` already reads
`PaintList::styles()` on the inspected main; it was not changed or counted as a
new fix. Native checks cover Windows at scale 1.0; other OSes, physical pointer
devices and other DPI factors have not been certified by this run. Existing
mixed-button gesture semantics are handled by the separate input-ownership
branch, not by this increment. No right-click menu or visual style was changed.

## Current integration verification

The 2026-10-19 collision review fetched main `452f659d6c92ebfe60554b21bd26f45e3829f5da`, inspected recent local branches and registered worktrees, and checked the open PR list. The original dock branch has no published PR. PR #340 covers compositor plane errors, #341 covers atlas lifetime, and #342 covers input/focus ownership. The active `feat/layer-render-sampling` branch was left untouched. The original dock worktree and root checkout were preserved. The carried implementation is commit `4317fd38` on `fix/dock-split-boundaries`.

The original five regressions were rerun with only `dock.rs` and `workspace.rs` restored to current main: **1 passed, 4 failed**, reproducing percentage minima, rejected-operation mutation, valid narrow-layout rejection and nonfinite input propagation. Evidence: `artifacts/issue-129-dock-main-before.log`. The additional sixth regression independently reproduced invisible nested-handle capture on main (**0 passed, 1 failed**), rather than stopping at layout validation. It checks all four parent orientations with distinct leaf identities. Evidence: `artifacts/issue-129-dock-nested-before.log`. All six passed on the repaired library with no ignored tests or weakened assertions: `artifacts/issue-129-dock-main-after.log`.

The rebuilt framework executable was copied to `artifacts/issue-129-dock/bin/framework.exe` before running native checks so another build cannot replace the executable under test. All eight localized cases passed:

| Case | Requested and reported logical size | Locales | Observed width | Scale |
| --- | --- | --- | --- | --- |
| Default | 1600 x 1000 | zh, en | 160 | 1.0 |
| Minimum | 900 x 600 | zh, en | 160 | 1.0 |
| Wide | 4000 x 1200 | zh, en | 160 | 1.0 |
| Existing stylesheet override | 1600 x 1000 | zh, en | 160 | 1.0 |

Reports and GPU captures are under `artifacts/issue-129-dock/{default,minimum,wide,override}/`. The actual reported logical sizes and every report flag were inspected. Visual inspection covered the Chinese minimum-window before/minimum captures and the English wide minimum capture. The Chinese minimum pair was shown to the user. These are standalone native UI framework captures using normalized input through actual workspace redraw and surface presentation; they do not claim physical-device automation or full-editor acceptance. Override cases also check successful and invalid stylesheet reloads. Native logs: `artifacts/issue-129-dock-native-{default,minimum,wide,override}.log`.

Static `git merge-tree` checks against #341 (`ad5c9218`) and #342 (`a82f7dfd`) found no conflicts. These establish textual compatibility only; no combined runtime validation is claimed.

Final verification completed successfully:

- `cargo test -j 1 -p motion-ui -p motion-desktop --locked -- --test-threads=1`: **686 passed, 0 failed, 1 existing native-audio test ignored**, exit 0. Desktop: 593 passed / 1 ignored, 284.37 seconds; UI: 93 passed / 0 ignored. The build completed in 6 minutes 40 seconds. Log: `artifacts/issue-129-dock-final-tests.log`
- `cargo build -j 1 -p motion-ui --example framework --locked`: passed in 26.81 seconds. Log: `artifacts/issue-129-dock-framework-build.log`
- `python -m unittest discover -s tools -p 'test_ci_*.py'`: **51 run, OK, 1 existing skip**, 41.009 seconds. Log: `artifacts/issue-129-dock-python-tests.log`
- `rustfmt --edition 2021 --check desktop/ui/examples/framework/dock_acceptance.rs desktop/ui/tests/dock_constraints.rs`: passed
- `python tools/validate_provenance.py`: passed (1445 tracked paths); panel style-consumer guard and `git diff --check`: passed; no panel files changed against current main

Rust commands used the portable MSVC environment and `CARGO_TARGET_DIR=E:/Dev/aem/target`; Python tools used the existing `.tools/build-validation-python` PyYAML environment. Each native run used `python tools/ci_ui_framework.py --binary E:/Dev/aem/artifacts/issue-129-dock/bin/framework.exe --output <case-directory> --size <requested-size>`; the override additionally used `--style desktop/ui/styles/example.override.json`. No existing assertion was weakened or removed and no new test has a skip path. PR CI remains a separate gate.

 The broad issue #129 remains open. This increment does not certify input allocations, grapheme/shaping behavior, scrub-field labels or the other remaining subitems.
