# Gradient primitive delivery

## Scope and status

This batch implements the first of the six proposed primitives: Gradient with Linear, Radial and Conic modes, editable endpoint/color/falloff socket defaults, explicit UV input and separate Float factor / Color outputs. Noise, generalized Blend, HSV Adjust, Color Ramp and UV Transform remain pending. Diamond is also pending. The broader node-editor usability goal remains active, including reusable groups and instance parameters, presets, naming/organization, conversion assistance, multi-error collection and NodeSpec.

The independently authored Three-node Gradient source contains UV, Gradient and Color Output, with two wires. A GPU regression compares it to an equivalent seven-node UV/axis/Dot/two-colors/Mix/Output graph. The original nine-node UV Component Gradient encodes U and V in separate channels; it is retained and is not an equivalent two-color replacement baseline. An earlier progress message conflated these examples; this inspection and the equivalent pixel comparison correct that claim. The source catalogue has 17 entries; its stable existing order and the Solid Color recommendation remain intact. No claim is made to have simplified every advanced graph or implemented the other five primitives.

## Behavior

- Searchable Linear, Radial and Conic variants create the same `gradient` operation with suitable initial defaults
- The native Mode dropdown switches explicitly, retains existing endpoints, colors and wires, and is undoable
- Following screenshot feedback, node choice menus attach to the clicked value field, measure their option text instead of using a fixed 246-pixel width, and flip upward near the canvas bottom. Multi-choice nodes retain the individual field anchor. Popup/row states and body text use the existing theme; `node.dropdown.row.height` is 28 logical pixels, retaining the previous 27-pixel base target at 100%. The first revision kept text and rows at fixed screen size and the user rejected its oversized appearance at 48% zoom. At their explicit request, the final menu shares the card's zoom for text, rows, padding, borders and shadows, with matching non-overlapping painted/hit geometry
- UV remains explicit; known pixel coordinates are rejected at UV inputs
- Linear projects start to end; radial measures distance from start against the endpoint span; conic uses the endpoint direction as zero and increases clockwise
- Falloff raises the clamped factor to an exponent bounded to 0.001..1000; endpoint spans at most 1e-6 UV and conic coordinates within 1e-6 UV of the center return zero
- Color interpolation follows the existing graph working-space and straight/premultiplied alpha contracts; authored color defaults remain straight RGBA
- Compilation emits one SDK 6 image pass with no graph texture samples or added resources; helper code is included only for reachable Gradient nodes
- Source save/reopen, factor preview and actual `.msfx` package loading are exercised; validated GLSL ES 300 is evidence of compilation, not Android device acceptance

## Verification evidence

Implementation commit: `692a85390211e3f7f83c0782cbb6d39aaaf0443d` on `codex/node-gradient`, based on `85b46d775a7e1d06fbdbecc7f4a2ef52eed7203c`. Final evidence is recorded under `E:/Dev/aem/artifacts/node-gradient`; `validation-summary.json` records the completion commit, changed-source hashes, executable fingerprint, screenshots and branch audit. The executable SHA-256 is `389d8bfd6db74817f7f13d5b5c00610731159dfdda1ee8986bf8a2df948fc158`.

| Check | Final result |
| --- | --- |
| `cargo check -p motion-desktop --offline` and debug build | Passed; existing compiler warnings retained |
| `cargo test -p motion-desktop --offline -- --test-threads=2` | 525 passed, 0 failed, 1 existing ignored; 118.30 seconds |
| Focused node card/control regressions | 17 passed, including 2 Gradient and 3 popup regressions |
| `cargo test -p motion-graph --offline -- --test-threads=2` | 50 passed, including 13 GPU tests; 6 new Gradient tests |
| `cargo test -p motion-ui --offline --test style` | 6 passed |
| Python tooling suite | 53 passed, 1 existing skip |
| Style-consumer guard, `git diff --check`, panels diff | Passed; no panels files changed |
| Native acceptance | Chinese and English at default and actual minimum node-window sizes; 48 checks per case, all passed; stdout empty in all cases |

The final native executable is `motion-studio.exe` in the artifact root. Latest synchronized-menu captures are `minimum/en/node-smoke.gradient-modes.png` and `minimum/zh/node-smoke.gradient-modes.png`; their actual-pixel crops are `comparison/scaled-en.png` and `comparison/scaled-zh.png`, listed with hashes in `comparison/latest-screenshots.json`. All four final menu screenshots were visually inspected. Use these latest captures for delivery; the superseded fixed-font captures remain in `validation/before-popup-zoom`.

The GPU regression compares every pixel of each mode against an independent CPU reference with rotated endpoints and non-unit falloff. Additional GPU cases cover coincident endpoints, the conic center and transparent color interpolation against independently constructed constant-color graphs. Desktop regressions exercise the actual dropdown, preserved wires/defaults, undo/redo, catalog creation, required UV diagnostics, wired-field rejection and source reopening. Three popup regressions cover attachment and edge flipping, individual fields on multi-choice nodes, and actual painted menu/glyph dimensions plus pointer selection / Escape / outside cancellation at 48%, 100% and 175% canvas zoom. Existing catalogue-count assertions change from 16 to 17 for the added source; no pre-existing behavioral assertion is weakened. The new popup regression is updated to guard the user's requested zoom behavior, preserving its logical base-size assertion

Native acceptance opens the real three-node example, captures all three installed GPU results, operates the real dropdown and numeric/vector fields through panel input routing, saves/reopens edited source, and verifies undo restores the prior GPU fingerprint. It preserves the prior 42 acceptance checks and adds six Gradient checks. Chinese and English runs use isolated saved layouts, unset `MOTION_LOG`, and verify empty stdout. Minimum runs resize the actual node floating window to 900×600; default runs retain 1280×800 node windows with a 1600×1000 main window.

## Design, provenance and limits

No panels files, dependencies, external assets or Adobe resources are added. Controls reuse existing input-default editing; the shared node choice popup is corrected using theme tokens, with its row-height token added to `dark.json` before use and its rules recorded in DESIGN.md. Gradient reuses the existing Color category appearance. Every DESIGN.md section 0.7 panel-remediation row is retained outside this batch, with no fix claimed; other node-canvas styling debt is likewise retained. Section 12 acceptance evidence covers this changed workflow, not all 19 panels or every DPI configuration.

UV radial distance intentionally has no automatic aspect correction. Conic has an intentional seam at its zero direction. This is a two-color gradient, not a multi-stop Color Ramp. Mode changes preserve user values, so switching from a linear graph does not reset its start to a centered radial default. Whole-graph fit at 900×600 uses a smaller canvas zoom; the existing zoom and fit-selected workflow is retained, and the screenshot evidence does not claim to solve all small-window text legibility. Mobile device comparison and sustained-performance acceptance remain pending.

The first native proof, captures before actual minimum floating-window resizing, four passing cases before the popup feedback, and the superseded fixed-font revision's four passing cases and desktop test log are retained in the artifact validation history. Before/after images are actual application captures, with crops under `comparison`. Machine-specific missing Epic Vulkan overlay JSON errors remain visible on stderr; no suppression or registry changes are part of this batch.
