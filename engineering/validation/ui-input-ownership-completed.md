# UI input ownership — issue #129 increment

Current base: main `452f659d6c92ebfe60554b21bd26f45e3829f5da`. Branch: `fix/ui-input-boundaries`.

The original implementation was commit `cf14bbef63f2b328c12dc69023da0b053cb1e72f` on `a801e54f155782647f69a361b71f32e560c10bf0`, in the preserved `fix/ui-input-ownership` worktree. Historical verification below belongs to that baseline; current integration evidence is recorded separately at the end.

This increment addresses mixed-button gestures and widget focus traversal. It does **not** close the broader #129 UI framework collection.

## Original collision review

Before implementation, fetched main, reviewed the complete open PR list and issue #129 timeline/comments, and inspected all 148 registered worktrees without changing them. Machine record: `.tools/p2-ui-input-branch-audit.json`. PR #329 already covers the desktop dimension-merge entry points, so that candidate was excluded. No open PR implements this input fix. The egui shell worktree reports `desktop/ui/tests/framework.rs` as modified without a substantive diff; this branch adds a separate `input_ownership.rs` test file and leaves that worktree untouched. No panel files, style tokens, dependencies or assets are changed.

## Delivered behavior

- Event accumulation preserves the first held pointer button, its origin and peak travel until the matching release, pointer exit or focus loss. Secondary press/release events remain observable to consumers without stealing the existing gesture.
- The current integration additionally prevents a later left press from arming NumberEditor/Button or taking Router capture/focus while a right/middle button already owns the gesture. Synthetic inputs with no held-button snapshot retain their existing behavior.
- NumberEditor only starts/updates from a held left button or a left-button release, and commits only on that release. Right/middle releases cannot begin an otherwise idle drag, commit it or remove its cancellation state.
- Button arming survives right/middle releases and still cancels on focus loss, Escape, disabled state or the primary release outside its bounds.
- Router retains primary capture through secondary releases and through events belonging to another native window. Capture still ends on its owner's left release or focus loss.
- Tab traversal deduplicates all hit regions by complete target identity, retaining first registration order. It stays within the active native window, visible clip and modal owner. Initial plain Tab / Shift+Tab presses navigate; IME, repeats and Ctrl/Alt chords do not. Shift direction is read from the actual key event.

## Original verification (historical baseline)

Eight new regression tests were run against the unchanged library first: **0 passed, 8 failed**, exposing real pointer/capture/focus failures. After the implementation, the same assertions passed: **8 passed, 0 failed**. Evidence: `.tools/p2-ui-input-before.log` and `.tools/p2-ui-input-after.log`.

The tests cover both primary and floating window IDs, both secondary buttons, pointer movement beyond the original field, exactly one numeric commit, cancellation to the original value, button cancellation, non-adjacent duplicate targets, reverse traversal, clipped/modal/foreign targets, per-event modifiers, IME/repeat/chord suppression, foreign-window release and all pairs of held/secondary buttons.

The complete motion-ui suite passed: **90 passed, 0 failed, 0 ignored**, including GPU pixel tests and all eight new regressions. The native framework example build and desktop check passed (desktop check: 42.39 seconds). Python tools ran **51 tests, 1 existing skip** in the repository's existing PyYAML-equipped environment. The system Python initially lacked PyYAML; no dependency or assertion was changed. Provenance passed for 1317 tracked paths, and the panel style guard passed.

Native validation passed all four cases: English and Chinese, each with the default stylesheet and the existing `example.override.json`. The override cases also retained the existing successful reload and invalid-reload rejection assertions. Reports and sixteen GPU captures are under `artifacts/ui-input-ownership/default/` and `artifacts/ui-input-ownership/override/`. Visual review confirmed the unchanged component presentation and the active numeric value in both a main window and a native float.

The complete serial desktop suite passed: **423 passed, 0 failed, 1 existing native-audio test ignored**, 372.04 seconds. No existing assertion was changed and no new test was ignored. Logs: `.tools/p2-ui-input-ui-tests.log`, `.tools/p2-ui-input-framework-build.log`, `.tools/p2-ui-input-desktop-check.log`, `.tools/p2-ui-input-desktop-tests.log`, `.tools/p2-ui-input-python.log`, `.tools/p2-ui-input-native.log` and `.tools/p2-ui-input-native-override.log`.

Commands (portable MSVC environment, `CARGO_BUILD_JOBS=2`):

```text
cargo test -p motion-ui --offline --locked --target-dir E:/Dev/aem/target -- --test-threads=1
cargo build -p motion-ui --example framework --offline --locked --target-dir E:/Dev/aem/target
cargo check -p motion-desktop --offline --locked --target-dir E:/Dev/aem/target
cargo test -p motion-desktop --offline --locked --target-dir E:/Dev/aem/target -- --test-threads=1
python tools/ci_ui_framework.py --binary E:/Dev/aem/target/debug/examples/framework.exe --output E:/Dev/aem/artifacts/ui-input-ownership/default
python tools/ci_ui_framework.py --binary E:/Dev/aem/target/debug/examples/framework.exe --output E:/Dev/aem/artifacts/ui-input-ownership/override --style desktop/ui/styles/example.override.json
python -m unittest discover -s tools -p 'test_ci_*.py'
python tools/validate_provenance.py
```

The existing native framework smoke now injects normalized events through the real native window's input, Router and PanelHost, presents each probe frame on its actual surface, and captures the active gesture on the GPU. Main-window verification commits one numeric drag and undoes it. Floating verification cancels an equivalent drag on focus loss without creating history. Both verify forward/reverse Tab and guarded repeat/Ctrl+Tab behavior. Report flags are set only after these assertions complete; `ci_ui_framework.py` requires those flags and the additional PNG captures.

## Issue #129 disposition

| Item | Disposition / evidence |
| --- | --- |
| NumberEditor secondary release | Fixed here; direct and routed numeric sequences |
| Button secondary release | Fixed here; release arming and cancellation regression |
| Router duplicate targets | Fixed here; stable unique forward/reverse traversal |
| Router window ownership / IME | Existing window filtering retained; capture release and key guards repaired here |
| Line clipping | Existing implementation clips the expanded stroke fringe instead of clamping centerline points; retained, not claimed as a new fix |
| Checkerboard size loop | Existing analytic pattern emits one quad with finite positive size validation; retained, not claimed as a new fix |
| Button key-repeat handling | Existing `key_presses` repeat guard retained |
| Atlas lifecycle, label bounds, GPU buffer growth | Outside this increment; not certified by these input tests; atlas work is separately proposed in PR #341 |
| Per-widget input cloning and drawing allocations | Pending separate performance work |
| Tabs stylesheet, split constraints, stroke geometry, grapheme editing | Outside this increment; no blanket completion claim |

## DESIGN.md §0.7 disposition

All baseline rows are retained because this patch changes input semantics without changing panel painting or geometry.

| Baseline item | Status / reason |
| --- | --- |
| Path literal vertical steps | Retained — no path row changes |
| Path two-column split | Retained — no path field sizing changes |
| Path mixed button heights | Retained — no control geometry changes |
| Path custom numeric background | Retained — numeric input behavior only, no drawing changes |
| Path boolean button | Retained — no boolean presentation changes |
| Effect row heights | Retained — no effect code changes |
| Effect boolean text buttons | Retained — no effect code changes |
| Effect enum full-width button | Retained — no enum changes |
| Effect manual hover alpha | Retained — no color changes |
| Effect indentation | Retained — no hierarchy changes |
| Timeline gutter coordinates | Retained — no timeline geometry changes |
| Timeline snap-guide color | Retained — no guide changes |

## Limits

Input still represents one gesture owner and one press/release snapshot per frame; this is not a general simultaneous-button event queue. Coalesced edge overwrite observations remain in place. Other consumers that inspect raw release snapshots must enforce their own gesture semantics. Native verification is on Windows; other platforms and physical devices are not certified by this run. This work does not expand the original #129 Unicode, atlas or allocation scope.

## Current integration verification

Revalidated on main `452f659d6c92ebfe60554b21bd26f45e3829f5da` in the isolated `fix/ui-input-boundaries` worktree. The original worktree and unrelated local changes were preserved. Open PRs #340 and #341 were inspected; neither implements input ownership. A Git merge-tree check against #341's `ad5c9218` tree found no conflicts; this is merge compatibility evidence, not a claim that a combined build was tested.

The eight existing regression tests were run with only the three production input files restored to the current main baseline: **0 passed, 8 failed**. Restoring the input implementation produced **8 passed, 0 failed** with unchanged assertions. Fresh logs are `artifacts/issue-129-input-main-before.log` and `artifacts/issue-129-input-main-after.log`.

Review then found an additional ownership inconsistency: holding Right/Middle and pressing Left preserved the event-layer owner but could still arm NumberEditor/Button and take Router capture/focus. A ninth routed regression reproduced this on the carried implementation (**0 passed, 1 failed**). The three consumers now reject that later Left press while another button is held. Legacy synthetic input snapshots with no held button remain accepted. All nine ownership regressions then passed. Evidence: `artifacts/issue-129-input-secondary-owner-before.log` and `artifacts/issue-129-input-secondary-owner-after.log`.

The native probe additionally exercises this secondary-first sequence in the main window and native float, checks unchanged model/history/focus/capture after every presented frame, and publishes `secondaryOwnerGuarded` only after those probes complete. The Python verifier requires the new flag. Both languages passed with the default theme and existing `example.override.json`; override runs also verified successful style reload and invalid-reload rejection. Reports and sixteen GPU captures are under `artifacts/issue-129-input/native-default/` and `artifacts/issue-129-input/native-override/`. Visual inspection covered the Chinese default-theme main/float active-gesture captures. The inspected adapter is NVIDIA GeForce RTX 4070 Laptop GPU, Vulkan. These are screenshots of the standalone native UI framework, with normalized events injected through its actual window input, Router and PanelHost; no physical mouse/keyboard automation or full-editor visual acceptance is implied.

Final full-suite verification completed successfully:

- `cargo test -j 1 -p motion-ui -p motion-desktop --locked -- --test-threads=1`: **689 passed, 0 failed, 1 existing native-audio test ignored** (desktop 593 passed / 1 ignored; UI 96 passed / 0 ignored), exit 0. Log: `artifacts/issue-129-input-final-tests.log`
- `cargo build -j 1 -p motion-ui --example framework --locked`: passed. Log: `artifacts/issue-129-input-framework-final-build.log`
- `python -m unittest discover -s tools -p 'test_ci_*.py'`: **51 run, OK, 1 existing skip**. Log: `artifacts/issue-129-input-final-python-tests.log`
- Native default/override verification: both passed, covering Chinese and English in each run. Logs: `artifacts/issue-129-input-native-default.log` and `artifacts/issue-129-input-native-override.log`
- Provenance validation, panel style consumer guard and `git diff --check`: passed. No panel file appears in the diff against the verified main baseline

Commands used the portable MSVC environment, shared `E:/Dev/aem/target`, and the existing `.tools/build-validation-python` PyYAML environment. The full desktop test run took 417.37 seconds; the combined build took 6 minutes 50 seconds. These local results do not substitute for PR CI.

 No existing assertion has been removed or weakened, and no new regression has a skip path. No panel files, tokens, dependencies or assets are changed. The wider issue #129 remains open; this increment does not certify atlas, Unicode, layout or allocation subitems.
