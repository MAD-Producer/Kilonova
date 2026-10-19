# UI text bounds and temporary allocation verification

## Scope and ownership

This increment addresses issue #129's long `ui::scrub_field` label and the text-related part of its per-control allocation item. It does not close the umbrella issue. Development started from main `452f659d6c92ebfe60554b21bd26f45e3829f5da` in the isolated `fix/ui-text-bounds` worktree. Open PRs and recent local branches were checked before implementation. The concurrent `feat/layer-render-sampling` branch concerns rendering quality and was not edited. The user subsequently confirmed that the work did not appear to overlap.

No panel, style token, dependency, asset, model, shortcut, menu or serialization file is changed. The legacy public `ui::scrub_field` currently has no production callers in this repository; its bounds fix is library correctness, not evidence of a visible editor change. The `style::text_style` allocation improvement applies to the active styled-text path.

## Changes

- Borrow the fitted UTF-8 text prefix and append an ellipsis during glyph iteration, avoiding temporary strings in drawing. Keep the public owned `TextAtlas::ellipsize` result and fitting semantics.
- Clip styled glyphs directly before inserting them in the destination list, replacing the temporary child list, clipping maps and append. Reuse the same glyph clipping calculation in `PaintList::clip_to`.
- Bound the scrub label to the existing left 45% and its value to the existing right 55%. Preserve its background, outline, rectangle, response and drag state.

These are functional bounds and allocation changes, not new error observations or a redesign. No claim is made about total application allocation count, frame rate or cold-cache performance.

## Regression evidence

The four new tests were run against the original production sources without changing their assertions: **1 passed, 3 failed**. Failures independently exposed label overlap in CPU glyph bounds and actual GPU pixels, and temporary allocations. The old styled-text clipping/order reference passed. Restoring the implementation produced **4 passed, 0 failed**. Logs: `artifacts/issue-129-text-before.log` and `artifacts/issue-129-text-after.log`.

The allocation test counts allocator calls on the measuring thread after warming glyph caches and output vector capacity. Each measured group contains one plain and one styled label. On the first short-label/width-64 case, the original path made 7 allocations for one group and 1400 for 200 groups, then failed the unchanged assertion. The implementation made 0 for both counts across all four cases: short and ellipsized labels at widths 64 and 240. This excludes numeric formatting, cold glyph rasterization and output capacity growth.

CPU tests cover widths 40, 100 and 220, idle and dragging states, and DPI ratios 1, 1.25, 1.5, 1.75 and 2. They verify label bounds, value visibility, unchanged hover/click/active responses, clipped glyph positions and UVs, and interleaved paint order for all three text alignments.

The GPU regression requires a real adapter and device and does not skip when they are unavailable. English and Chinese long labels are captured at all five DPI ratios. The value region must match a value-only reference exactly. An independent old full-row label control must contaminate more than five pixels, preventing blank rendering from passing. Styled text must match the legacy clipping path exactly for five ratios and three alignments, including vertical clipping and oversized font glyphs. Captures are under `artifacts/issue-129-text/before/` and `after/`; these are offscreen controls, not full-editor screenshots or physical input automation.

The native standalone UI framework passed English and Chinese with the default stylesheet and the existing override stylesheet, including actual main and floating windows, numeric undo and layout round-trip. Override runs also verify successful reload and invalid-reload rejection. Reports and captures are under `artifacts/issue-129-text/native-default/` and `native-override/`. The probe uses injected normalized events; it does not establish physical keyboard/mouse or whole-editor acceptance.

## Final integration

The complete suite on the original main baseline plus this patch passed: **684 passed, 0 failed, 1 existing native-audio test ignored** (desktop 593 passed / 1 ignored; UI 91 passed). Evidence: `artifacts/issue-129-text-final-tests.log`. A separate comparison of all fifteen saved style captures from the original and modified production sources found identical RGBA pixels.

PRs #340, #341 and #342 merged during verification. The isolated branch was updated to main `be0432559cb9bf26f9833397cc2f9283773005f1`, and the patch reapplied without conflicts. Review confirmed that glyphs retain #341's `atlas_size` metadata and that the new clipping helper copies it unchanged.

On that integrated baseline, all **105 UI tests passed, 0 failed, 0 ignored**. The fifteen GPU style captures again match the original production pixels exactly; sizes, RGBA hashes and equality results are retained in `artifacts/issue-129-text/integrated-pixel-comparison.json`. The framework example build and all four native locale/theme cases passed, including #342's gesture-owner and Tab navigation assertions. Visual inspection covered the real Chinese framework window and both languages' bounded label captures. The inspected adapter is NVIDIA GeForce RTX 4070 Laptop GPU, Vulkan. Integrated evidence: `artifacts/issue-129-text-integrated-ui.log`, `issue-129-text-integrated-framework.log`, `issue-129-text/integrated-after/`, `integrated-native-default/` and `integrated-native-override/`.

The integrated desktop suite also passed: **593 passed, 0 failed, 1 existing native-audio test ignored**, in 209.53 seconds. The combined integrated result is **698 passed, 0 failed, 1 existing ignore**. Evidence: `artifacts/issue-129-text-integrated-desktop.log`. Both Cargo commands and the native verification process exited successfully.

Commands (portable MSVC environment, shared target directory, one Cargo build job):

```text
cargo test -j 1 -p motion-ui --locked -- --test-threads=1
cargo build -j 1 -p motion-ui --example framework --locked
cargo test -j 1 -p motion-desktop --locked -- --test-threads=1
python tools/ci_ui_framework.py --binary E:/Dev/aem/artifacts/issue-129-text/bin/framework.exe --output E:/Dev/aem/artifacts/issue-129-text/integrated-native-default
python tools/ci_ui_framework.py --binary E:/Dev/aem/artifacts/issue-129-text/bin/framework.exe --output E:/Dev/aem/artifacts/issue-129-text/integrated-native-override --style desktop/ui/styles/example.override.json
python tools/validate_provenance.py
python -c "import sys; sys.path.insert(0,'tools'); import ci_desktop; ci_desktop.verify_style_consumers()"
rustfmt --edition 2021 --check desktop/ui/tests/text_bounds.rs desktop/ui/tests/text_bounds/gpu.rs
git diff --cached --check
```

Provenance passed for 1452 tracked paths after staging the new tests and this record. The panel style guard, new-test formatting and diff whitespace checks passed. The panels diff is empty. Existing assertions were not changed to make acceptance pass. The new tests initially exposed a transparent ordering fixture and a missing CJK fallback in the fixture; both were corrected before the final baseline comparison, without weakening an assertion or changing product font selection.

## DESIGN.md disposition

The change retains the row contract and hit areas. No panels are edited. All twelve §0.7 baseline rows are retained with reason; this does not certify their existing presentation:

| Baseline item | Status / reason |
| --- | --- |
| Path literal vertical steps | Retained — no path row changes |
| Path two-column split | Retained — no panel sizing changes |
| Path mixed button heights | Retained — no control geometry changes |
| Path custom numeric background | Retained — no panel numeric rendering changes |
| Path boolean button | Retained — no boolean changes |
| Effect row heights | Retained — no effect code changes |
| Effect boolean text buttons | Retained — no effect code changes |
| Effect enum full-width button | Retained — no enum or menu changes |
| Effect manual hover alpha | Retained — no color changes |
| Effect indentation | Retained — no hierarchy changes |
| Timeline gutter coordinates | Retained — no timeline geometry changes |
| Timeline snap-guide color | Retained — no guide changes |

## Remaining limits

Input routing still clones owned input data. Icon geometry and triangle clipping allocations, numeric formatting and cold output-buffer growth are outside this increment. Grapheme editing, shaping and combining-mark layout are not implemented by borrowing a UTF-8 prefix. The independent atlas, input and dock work has separate acceptance records. No blanket completion claim is made for issues #129, #126 or #134.
