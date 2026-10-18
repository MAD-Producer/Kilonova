# Node diagnostic drawer acceptance

Status: A5 implementation and scoped acceptance complete; other node usability items remain pending

## Scope and coordination

- Audit item A5: expand the current diagnostic, expose its full cause and navigate to the affected node
- Branch `codex/node-diagnostics`; implementation commit `fd723e5`; previous wire-targeting commit `f658134` preserved
- Read-only branch/worktree/PR audit: `2026-10-18T12:27:42.640934+00:00`; an explicit fetch advanced main to `01450571`, including #331–#334
- Main integration commit `140d97a` preserves the View menu, minimap event ownership, compatible-port chooser and language-aware wire drops. Other dirty worktrees were left untouched
- No subagents, new dependencies or third-party assets

## Behavior and implementation

- `view/diagnostics.rs` draws a footer Details button and a bottom drawer using existing panel/header/row/padding/border/error tokens
- The drawer labels code, qualified node and port separately, wraps the complete original cause (including paths without whitespace), and scrolls with wheel, arrows, Page Up/Down and Home/End
- Locate node / Enter resolves the real qualified ID through nested groups and centers the selected node above the drawer. Source, folded state and undo history remain unchanged. Scope transitions reuse normal group navigation and its preview-target reset behavior
- Unresolvable targets disable location. Active fields, menus and gestures retain priority; drawer focus suppresses canvas deletion. Esc closes details; changed diagnostics reset the scroll episode and successful validation clears the drawer
- On the current diagnostic node, hollow error rings mark all inputs lacking both an incoming edge and a saved default. Connected/defaulted inputs and other unfinished draft nodes stay unmarked
- Pointer occlusion preserves active text/number editor keys and cancellation; during a wire drag, the first Esc cancels that drag while keeping details open
- The drawer captures its opaque pointer region. Releasing a wire or live numeric drag over it cancels that gesture instead of completing through covered controls
- Only the existing single validation diagnostic is shown. Routine status notices are still footer notices; this does not implement multi-error collection or structured action-error history

## Evidence

- `cargo check -p motion-desktop --offline`: passed (21 existing warnings)
- `cargo test -p motion-desktop --offline -- --test-threads=2`: 510 passed, 0 failed, 1 ignored, including eight new diagnostic regression tests; final elapsed test time 105.85 s
- The ignored test requires an actual native audio output device and was already ignored before this change
- Python tool suite: 54 run, 53 passed, one platform-specific skip; style-consumer guard and `git diff --check` passed
- Four native cases at default `MOTION_LOG` (unset): default 1600×1000 / minimum 900×600 main window × Chinese/English, each with 36 required checks true and zero stdout bytes
- Native case times: default/zh 81.01 s, default/en 76.92 s, minimum/zh 53.81 s, minimum/en 57.06 s
- Real MissingInput from disconnecting Output.color: actual pointer opens details, actual pointer locates the previously offscreen output, source/revision/preview stamp are unchanged, retained GPU fingerprint is unchanged, and actual Escape closes the drawer
- Legacy GPU/source/package/history/default-input/wire checks all remain mandatory. Main and floating grid/minimap menus still save and reopen correctly. The retained UI probe still records one render and eight reuses at the fixed Balanced fixture tier
- Unit coverage includes full Unicode cause reconstruction and scrolling/reset, nested group navigation with collapsed state and edited child data preserved, unresolved IDs, valid defaults/connections, narrow layouts in both languages, modal ownership, destructive-key isolation and wire/numeric release cancellation
- Executable SHA-256: `8869fdeada3cee8001e2d394a84aff342768ebcad562a4886f47adf264373069`
- Inspected actual default Chinese/English located screenshots and the minimum Chinese closed screenshot. The review images are direct native crops, not mockups; the original full windows remain in each case folder
- Reviewed screenshot: `default/zh/node-smoke.diagnostic-located.png`; companion English screenshot: `default/en/node-smoke.diagnostic-located.png`; close state: `minimum/zh/node-smoke.diagnostic-closed.png`
- Full records, initial failure evidence, run script, source hashes, executable hash and per-case assertions are retained in `validation-summary.json` and `validation/`

Artifacts: `E:/Dev/aem/artifacts/node-diagnostics/`

## Initial failures and verification limits

- One initial native run used the inherited user layout while the desktop suite spawned additional application processes. The floating-window workspace reload assertion failed. The acceptance runner now sets a distinct `MOTION_UI_LAYOUT` per case; initial failure output is retained
- Two default-parallel desktop runs timed out in the existing `effect_import::tests::installed_package_is_logged_before_a_failed_catalogue_refresh` test. Captured evidence includes a 6253 ms caught-panic path against its unchanged 5 second reply assertion. Another worktree was concurrently building an optimized desktop binary. The final suite limits test scheduling to two threads; assertions, timeouts and tests are unchanged
- An isolated-layout native run then exposed the old default-input fixture clicking fields outside the visible canvas. The fixture now explicitly sets its view before those unchanged pointer/source/GPU assertions, and its read-only geometry helper rejects offscreen field centers. This changes acceptance setup only, not normal editor interaction
- Native verification is run separately from the desktop suite. This scheduling/environment change is distinct from product behavior
- Unit coverage exercises 320/360/600 px editor widths and both languages. Four native cases cover default/minimum main-window sizes and both languages, with a 1280×800 node floating window at the current Windows display scale. This does not establish arbitrary native DPI or minimum-width floating-window acceptance
- Existing Windows Vulkan loader messages about missing Epic overlay manifests remain visible on stderr
- DESIGN.md §0.7 panel remediation rows are retained outside this node-only batch. No panel source was edited. Existing node category/type colors remain; this change replaces the current diagnostic border/footer color with the existing error token

## Remaining node usability work

A4 onboarding, snippets/presets, reusable named groups with instance parameters, multi-diagnostic collection (B7), broader organization/naming and catalog/NodeSpec consolidation remain separate work. A1, A2/A3 and B1 evidence is historical; their required native checks are rerun here without claiming them as new implementations
