# Node first-use guidance (A4)

## Scope

- Add a non-modal welcome strip to fresh standalone demos and an empty-state starting point to standalone root canvases
- Identify the existing animation demo explicitly, offer the existing Examples flow, and preserve source, history, selection, preview and node screen positions when Continue is clicked
- Recommend an independently authored Solid Color example containing two nodes and one wire; keep the existing default demo and catalogue order stable, and promote the starter only in the browser's presentation order
- Retain the existing save/discard choices, stale-snapshot rejection, graph formats, layer-effect identity graphs and nested-group behavior
- Keep popups and the diagnostic drawer ahead of welcome input; locate diagnostic nodes inside the remaining canvas beneath the welcome strip

## Acceptance

Verification results and native screenshots are recorded in `E:/Dev/aem/artifacts/node-onboarding/validation-summary.json`. The native runner uses isolated workspace paths, the default log level, and the same built executable for each case. Compilation alone does not constitute acceptance.

- Implementation commit: `7332243` (`feat(nodes): 添加首次上手引导与双节点纯色示例`)
- Final offline desktop check and build passed; the final desktop suite passed 520 tests, failed none, and ignored one existing real-audio-device test, with two test threads
- All nine focused onboarding tests passed; the full suite also includes the host save/stale-guard regression, for ten new tests in this batch
- Graph example and port suites passed six tests; Python tooling passed 53 tests with one skip; the style-consumer guard and diff whitespace checks passed
- Native default/Chinese, default/English, minimum/Chinese and minimum/English cases each passed all 42 checks, with zero stdout bytes in every case
- Executable SHA-256: `3101cb517c9f4198b9368286851596e5fc0473a4c1937bebd5cd4d8d067f355e`

The native stderr retains this machine's missing Epic Vulkan overlay JSON loader errors. No system registry changes or suppression were applied. Initial fixture failures and the earlier preview-synchronization capture are retained under the artifact validation directory. No files under `desktop/app/src/panels` changed.

Native coverage includes welcome, dismissal, empty canvas, the actual Examples action and browser Open button, real GPU rendering of the two-node starter, changed pixels after editing Color, source reopening, undo restoring pixels, and an unchanged user project. The prior draft/default-input/wire/diagnostic acceptance checks remain required.

New regression tests cover observational welcome actions, stable positions after dismissal, loaded/bound/nested scopes, narrow docks, both languages, popup priority, the recommended browser selection, edit/undo, save/stale guards, diagnostic centering, overlapping drawer input, and stable first-node drop coordinates through undo/redo. The two catalogue-size assertions change from 15 to 16 because one source example was added; existing behavioral assertions remain intact.

## Review findings

- An initial dismissal test found that the newly exposed canvas received the same click and cleared selection. Welcome clicks are now consumed before canvas input, and pan compensates for the removed strip
- The additional graph suite found the existing invariant that catalogue entry zero equals the default demo. Keep that invariant and stable catalogue order; reorder recommendations only in the browser
- Empty-root guidance does not reserve additional canvas space, so adding the first node preserves its intended drop position and history
- Diagnostic centering now uses the guided canvas bounds, preventing the new strip from shifting a located node below its intended visible area
- The first native view-menu check clicked the reserved welcome strip using the old reconstructed canvas geometry. The acceptance path now reads the actual last-drawn canvas bounds; its menu assertions remain unchanged
- Screenshot review found the late first-use fixture still displaying the previously retained preview after its editor was reset. Wait for the reset demo's actual GPU install before capturing the welcome state; keep the old diagnostic-retention checks intact

## Design and provenance

Use existing panel, text, muted, row-height, font and padding tokens. No panel files, dependencies, external assets, Adobe resources or copied product text are added. DESIGN.md section 0.7 panel remediation rows are retained outside this batch, not claimed fixed. Existing node-canvas styling debt is also retained. DESIGN.md section 12 evidence is scoped to the changed node workflow, not a claim that every desktop acceptance route was rerun.

## Remaining limits

The welcome state is per editor instance, not an installation-wide tutorial preference. Non-empty loaded documents have no welcome strip; empty standalone loaded roots retain the empty-state action. Small docks keep the existing toolbar Examples entry. Layer effects and nested groups do not receive standalone starter replacement actions.

This completes the first-use/empty-state/example-entry batch. Search metadata, reusable groups with instance parameters, preset assets, graph organization, multi-error collection and the NodeSpec refactor remain separate work. A recommended simple example does not reduce the node count inside existing advanced graphs.

The six previously proposed primitives (Gradient, Noise, generalized Blend, HSV Adjust, Color Ramp and UV Transform) are not implemented by this batch or the preceding usability batches. Solid Color is an example built from existing Color and ColorOutput operations, not a new node operation. Existing UV/math building blocks and Mix/Multiply/Screen operations are not completion evidence for those six primitives.
