# Borrowed input routing verification

## Scope and ownership

This increment addresses issue #129's owned input cloning in the shared Router, PanelFrame and widget filters. The isolated `perf/ui-borrowed-input` branch starts from main `be0432559cb9bf26f9833397cc2f9283773005f1`. Recent branches and open PRs were checked first: #343 changes dock constraints, #344 changes layer sampling, #345 changes text allocation/bounds and #346 changes icon geometry. None implements borrowed routing. Those worktrees and unrelated root files were preserved.

No panel, style token, asset or dependency changes. This is an input data-path optimization, not new error observations or an interaction redesign. It does not close the umbrella issue or certify issues #126/#134 complete.

## Implementation and API boundary

- Retain owned `Input` for normalized event accumulation. Add `InputView<'a>`, a Copy snapshot whose scalar fields can be independently masked while text and all four key vectors are borrowed slices.
- Add `InputSource` for shared read-only controls. Existing calls passing `&Input` continue to work, and routed/clipped views can pass directly through buttons, numeric/text editors, composites, icon buttons, tabs and numeric cursor policy.
- Return borrowed views from `Router::input_for` and `PanelFrame::input_for`. Borrow only the source input, allowing a panel to retain its routed view while mutating its paint list and atlas.
- Replace deep copies in button visual filtering, search-field key masking, generic clipping and borrowed scroll child-input filtering. Retain their original masks and pointer-release semantics.
- Borrow the node inspector's local clipped input too. Its existing mouse-position mask is intentionally retained; it is not replaced with the shared helper's different press-position/wheel mask.
- Share `key_presses` semantics between owned input and the view, including synthetic missing repeat flags and IME commit suppression.

The return-type change is a public toolkit source-API change: `input_for` and `clipped_input` expose borrowed views instead of owned snapshots. Read-only consumers compile through `InputSource`; consumers that formerly pushed into returned vectors or retained them past the source input must adapt. This is not a JSON/MCP, project format or shortcut contract change. No unsafe lifetime conversion or payload ownership escape is used. A compile-fail doctest checks that the view cannot outlive its source.

Desktop compilation identified panel helpers that require `ScrollFrame` to retain an owned input. The user's panel restriction remains in force, so the existing `ScrollArea::begin` retains that ownership boundary, using an explicit `InputView::to_owned` conversion. New `begin_view` returns a borrowed frame and is used inside the shared ListView. `ScrollFrame<I = Input>` keeps the existing default type. This is an acknowledged remaining per-panel copy, not a claim that all scroll input now borrows. A regression compares the complete input masks, geometry and offsets of the two entry points and checks ownership addresses.

## Baseline failure and targeted evidence

The initial three new regressions were run on unchanged main production code: **0 passed, 3 failed**. The focused-owner routing loop and clipped-input loop each made 1000 allocations for 200 operations with nonempty key vectors and a repeated Unicode text payload. A separate address assertion proved that the old route copied the text rather than borrowing it. Assertions were not changed to accept allocations or copies.

With the implementation, the same three tests passed. Both focused and unfocused 200-route loops and the 200-clip loop made **0 allocations**. Address checks cover text, key presses, repeat flags, held keys and released keys; nested clipping preserves the captured release and held button without modifying the original payload. Logs: `artifacts/issue-129-input-view-before.log` and `issue-129-input-view-after.log`.

Additional regressions compare every view field and key iterator result to the legacy clone-and-mask reference for two windows, modal/nonmodal state, focus loss, composition and IME consumption. A real PanelHost/PanelFrame probe performs 200 routes inside its draw callback and requires zero payload allocations. The node inspector regression verifies borrowed payload addresses and its original inside/outside pointer mask while preserving wheel, release and held state. Existing interaction assertions are retained.

The zero-allocation measurements cover routing and filtering, not text editing buffers, widget responses, painting, glyph rasterization, event accumulation or the whole application. Copying the bounded view still copies scalar fields and slice descriptors; no total CPU/frame-rate improvement is claimed.

## Final acceptance

The final dedicated-target UI suite passed: **108 passed, 0 failed, 0 ignored**, including the six borrowed-input regressions, nine existing ownership regressions and the compile-fail lifetime doctest. The scroll regression independently checks masks, viewport dimensions, offset and content geometry in addition to comparing the borrowed and owned entry points. Its initial independent wheel expectation incorrectly forwarded wheel input to the child; inspection of main confirmed that the scroll container always consumes it. The new fixture was corrected to that legacy rule, with all geometry, address and ownership assertions retained and no product change. The strengthened complete UI suite then passed. The framework example also built successfully. Final logs: `artifacts/issue-129-input-view-isolated-accepted-ui.log` and `issue-129-input-view-isolated-framework.log`. The earlier failed fixture run remains in `issue-129-input-view-isolated-final-ui.log`.

The isolated binary passed all four English/Chinese and default/override native cases. Existing assertions cover captured numeric drags, one undo, focus-loss cancellation, secondary-button ownership, Tab guards and layout round-trip. Overrides additionally cover successful reload and invalid-reload rejection. All eight main/float captures matched the prior icon branch's acceptance images exactly; that branch had already established pixel equivalence to this main baseline. Hashes and equality results are retained in `artifacts/issue-129-input-view/isolated-native-pixel-comparison.json`; reports and captures are under `isolated-native-default/` and `isolated-native-override/`. The binary is copied to `artifacts/issue-129-input-view/bin/framework.exe` before native runs so subsequent compiles cannot replace it. The adapter is NVIDIA GeForce RTX 4070 Laptop GPU, Vulkan. These are standalone framework windows with injected normalized events, not physical input or whole-editor visual certification.

The dedicated-target desktop suite passed: **594 passed, 0 failed, 1 existing native-audio test ignored**, in 232.01 seconds. The new inspector borrowing regression passed in that full run. Combined acceptance is **702 passed, 0 failed, 1 existing ignore**. The desktop process exited successfully. Evidence: `artifacts/issue-129-input-view-isolated-desktop.log`.

Provenance passed for 1451 tracked paths after staging the new test and record. The panel style guard, new-test formatting and staged whitespace checks passed; the panels diff is empty. Main advanced to `015df56` (the separate dock PR #343) during verification; this branch's measurements are explicitly on `be04325` plus this patch, not a claimed combined runtime result.

During the related font-fixture follow-up, a shared-target run compiled the new integration test but executed library behavior matching an older worktree (7/1400 text allocations and uncropped labels), despite the inspected source containing the fix. The shared dep-info records use relative source paths and the library filenames collide across worktrees. That contradictory result is not used for acceptance. Final validation uses a dedicated target directory so another worktree cannot replace or supply its library artifacts. No test was restarted merely for an observation timeout: the contradictory shared-target run had exited with failures before isolated validation began.

Final commands use the portable MSVC environment and `CARGO_TARGET_DIR=E:/Dev/aem/target/observability-input-validation`:

```text
cargo test -j 1 -p motion-ui --locked -- --test-threads=1
cargo build -j 1 -p motion-ui --example framework --locked
cargo test -j 1 -p motion-desktop --locked -- --test-threads=1
python tools/ci_ui_framework.py --binary E:/Dev/aem/artifacts/issue-129-input-view/bin/framework.exe --output E:/Dev/aem/artifacts/issue-129-input-view/isolated-native-default
python tools/ci_ui_framework.py --binary E:/Dev/aem/artifacts/issue-129-input-view/bin/framework.exe --output E:/Dev/aem/artifacts/issue-129-input-view/isolated-native-override --style desktop/ui/styles/example.override.json
python tools/validate_provenance.py
rustfmt --edition 2021 --check desktop/ui/tests/borrowed_input.rs
git diff --cached --check
```

## DESIGN.md disposition

All twelve §0.7 baseline rows are retained with reason: no path row steps, two-column split, control heights, numeric backgrounds or boolean presentation changes; no effect row heights, boolean/enum buttons, hover alpha or indentation changes; no timeline gutter or snap-guide changes. None is certified compliant by this patch. Row geometry, control states and hit areas are preserved. No context menu changes.

## Remaining limits

Some application-specific owned snapshots remain: native framework per-window redraw snapshots, shell acknowledged input, node rename/search bounds, node numeric command synthesis and panel-local input masks. Those are distinct sites; this patch does not claim to eliminate them. Graph/editor text changes still allocate owned editing strings. Geometry/list allocations, grapheme editing and layout shaping remain separate #129 work. Existing lifetime, ordering and rendering acceptance records in #341/#342 and the pending #343/#345/#346 proposals must not be conflated with this branch's results.
