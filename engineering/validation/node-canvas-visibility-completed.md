# Node canvas view controls — issue #267

## Scope and collision check

- Issue: https://github.com/MAD-Producer/Kilonova/issues/267
- Branch: `feat/node-canvas-visibility`
- Upstream base: `a801e54f155782647f69a361b71f32e560c10bf0`
- Before implementation, the 2026-10-18 audit checked all 146 local worktrees, recent branches, open PRs and the issue timeline. No linked implementation PR for #267 was present.
- PR #327 (`feat/desktop-io-columns-hover`) owns timeline columns, IO shortcuts and handle hover. PR #329 (`fix/desktop-property-followups`) owns property/mask/expression follow-ups. `fix/desktop-interaction-regressions` also has local timeline/navigation changes. This branch leaves their panel implementations untouched.
- The raw local branch audit is `.tools/p2-branch-audit.json`; it is an environment-specific record, not a distributed product asset.

## Implemented behavior

- Add exact, non-repeating G/N bindings in node scope. Preserve Ctrl+G grouping and Alt+G ungrouping. Text/IME, popups, focus and active gestures retain precedence.
- Expose checked grid/minimap choices in the View toolbar menu and the canvas context menu, in Chinese and English.
- Keep the grid visible and the minimap hidden by default. Store both preferences in the workspace layout; accept legacy layouts without these fields. Retain current preferences across graph loads and bound/draft transitions.
- Project the viewport and node extents through one aspect-preserving minimap transform. Recompute after pan/zoom; freeze the transform during click/drag navigation to prevent feedback drift. Esc, focus loss and missing release restore the previous pan. Hide the overlay when the canvas is too small, without resetting the preference.
- Keep navigation and visibility changes outside graph data, compile stamps, permanent output and undo history. Save only workspace preferences when a toggle changes.
- Add semantic minimap metrics to `dark.json`; reuse existing native colors and checkbox drawing. No new dependency or third-party asset is introduced.

## Validation

Run date: 2026-10-18, Windows/MSVC, two Cargo build jobs. No existing test assertion was relaxed.

| Check | Result |
| --- | --- |
| `cargo check -p motion-desktop --offline --locked` | Passed on the final source |
| `cargo build -p motion-desktop --offline --locked` | Passed on the final source |
| `cargo test -p motion-desktop --offline --locked -- --test-threads=1` | Two full runs passed: 430 passed, 1 existing native-audio test ignored; the second run did not enable documentation regeneration |
| `cargo test -p motion-desktop --offline --locked view_preferences -- --test-threads=1` | Passed: 7 tests on the final source, after the native-fixture refinement |
| `python -m unittest discover -s tools -p 'test_ci_*.py'` | Passed: 51 tests, 1 existing skip; used the existing validation Python environment with PyYAML |
| `python tools/validate_provenance.py` | Passed, including the newly staged source and validation files |
| `ci_desktop.verify_style_consumers()` and Python compilation | Passed; no panel file changed |
| `git diff --cached --check` | Passed |

The new tests cover exact scopes/modifiers/repeats, text/IME/focus/menu/gesture precedence, history/compile/permanent-source isolation, nonzero canvas origins, pan/zoom and empty/invalid/small views, click/drag navigation with a frozen transform, cancellation/recovery, checked menus in both languages, workspace save/reopen and legacy defaults.

The native `--node-smoke` fixture captures the real main and floating renderers, toggles G/N, cancels a canvas menu and activates its grid entry, verifies project/compile identity, and reloads workspace preferences. CI now requires these records and their rendered PNGs. The fixture explicitly switches to the Nodes workspace for the main-window probe, then restores Default before continuing the existing graph acceptance. It also creates an empty canvas target before right-clicking, so the minimum-window probe cannot accidentally open a node menu.

Final native run matrix, after the menu-target refinement:

| Locale / main window | Native result | Duration |
| --- | --- | --- |
| English / 1600×1000 | Passed, main and float | 109.24 s |
| Chinese / 1600×1000 | Passed, main and float | 98.36 s |
| English / 900×600 | Passed, main and float | 69.84 s |
| Chinese / 900×600 | Passed, main and float | 68.31 s |

Each report confirms `projectAndCompileUnchanged`, `workspaceReload`, `menuCancelled` and `menuToggled` for both windows. The existing GPU graph checks also pass, including time/parameter pixel changes, grouping/ungrouping output preservation, invalid-graph recovery, project undo/redo and offline package reopen. Main/floating screenshots and the minimum-window canvas menu were visually inspected. This is local acceptance; no remote PR/CI result is claimed.

Local evidence lives under `E:/Dev/aem/artifacts/node-canvas-visibility`: `<locale>-<default|minimum>.json`, stderr/stdout logs, and `*.node-view-{main|float}-{overview|no-grid|menu}.png`. Command logs are `.tools/p2-node-{check,build,tests-final,tests-view-final,python-tests,native-validation}.log`. These are regenerable local artifacts, not product assets.

To reproduce, build the desktop binary, set `MOTION_UI_LAYOUT` to an isolated layout path, and run `motion-studio --node-smoke <report.json> --locale en` or `zh`. Repeat with `MOTION_SMOKE_MINIMUM=1` for the minimum window. The local run used the shared build target `E:/Dev/aem/target`.

Limits: native captures use this machine's 100% Windows display scale. Other OS display scales and moving a live window between monitors were not exercised. A narrow canvas can intentionally omit the minimap while retaining its enabled preference. Existing Vulkan overlay-registration and font fallback diagnostics can still appear in smoke stderr; this issue does not change that infrastructure or install system fonts.

## Design baseline disposition

The scope does not edit `desktop/app/src/panels/**`. Every DESIGN.md §0.7 baseline item is retained with reason: the six path-row steps, two-column split, mixed control heights, numeric drawing, mask boolean, effect row heights, effect boolean/enum, hover background, effect hierarchy, timeline column coordinates and snap-guide color belong to separate panel remediation. This change does not claim those existing issues are fixed or that the complete UI specification is compliant.

The node toolbar/context menu retain their existing layout grammar. New minimap sizes and viewport fill opacity use semantic tokens. Global panel restyling, shortcut rebinding and OS input-method conflicts are outside #267.
