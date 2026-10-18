# UI input ownership — issue #129 increment

Base: `a801e54f155782647f69a361b71f32e560c10bf0` (`origin/main`). Branch: `fix/ui-input-ownership`.

This increment addresses mixed-button gestures and widget focus traversal. It does **not** close the broader #129 UI framework collection.

## Collision review

Before implementation, fetched main, reviewed the complete open PR list and issue #129 timeline/comments, and inspected all 148 registered worktrees without changing them. Machine record: `.tools/p2-ui-input-branch-audit.json`. PR #329 already covers the desktop dimension-merge entry points, so that candidate was excluded. No open PR implements this input fix. The egui shell worktree reports `desktop/ui/tests/framework.rs` as modified without a substantive diff; this branch adds a separate `input_ownership.rs` test file and leaves that worktree untouched. No panel files, style tokens, dependencies or assets are changed.

## Delivered behavior

- Event accumulation preserves the first held pointer button, its origin and peak travel until the matching release, pointer exit or focus loss. Secondary press/release events remain observable to consumers without stealing the existing gesture.
- NumberEditor only starts/updates from a held left button or a left-button release, and commits only on that release. Right/middle releases cannot begin an otherwise idle drag, commit it or remove its cancellation state.
- Button arming survives right/middle releases and still cancels on focus loss, Escape, disabled state or the primary release outside its bounds.
- Router retains primary capture through secondary releases and through events belonging to another native window. Capture still ends on its owner's left release or focus loss.
- Tab traversal deduplicates all hit regions by complete target identity, retaining first registration order. It stays within the active native window, visible clip and modal owner. Initial plain Tab / Shift+Tab presses navigate; IME, repeats and Ctrl/Alt chords do not. Shift direction is read from the actual key event.

## Reproducible verification

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
| Atlas lifecycle, label bounds, GPU buffer growth | Outside this increment; not certified by these input tests |
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
