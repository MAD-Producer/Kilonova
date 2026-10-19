# Node wire targeting acceptance

Status: A2/A3 implementation and scoped acceptance complete. Other audit items remain pending.

## Scope and coordination

- Audit items: A2, feedback and compatible-port selection on node body drops;
  A3, nearest socket hit testing shared by hover, press and release.
- Branch: `fix/node-wire-targeting`; main base: `6fce5769` (#330).
- Read-only branch/worktree/open-PR audit: `2026-10-18T11:21:52.971738+00:00`.
  The audit recorded main at `8d6698a5`; an explicit fetch then advanced it to
  `6fce5769` before this branch was created. A1 and B1 were cherry-picked cleanly
  as `ca53f62` and `e8667a1`. Their original acceptance documents remain historical
  records. They are not new A2/A3 evidence.
- Checked PRs: #331 node canvas visibility, #332 temporary panel maximize,
  #333 property keyframe navigation and #334 UI vertex buffer growth. No parallel
  wire-targeting implementation was found. Existing integration branches and
  other dirty worktrees were preserved. No subagents were used.
- The independent acceptance setup fixes are committed as `aa5d088`.

## Changed behavior

Nearest screen-space socket selection preserves the old 9px hover extent and
expands to include the visible socket edge at high zoom. Ties prefer the front
card. Hidden sockets under front cards and sockets completely clipped outside
the canvas do not intercept gestures. Hover, click and release share this
geometry; dense low-zoom inputs no longer select the first traversed socket.

Body drops work in both directions. One fully valid candidate connects;
multiple candidates open a nearest-first port chooser. The chooser uses existing
context menu styles and supports pointer selection, wheel, Up/Down, Home/End and
Enter. Long authored group port names are clipped inside the menu; a large port
list scrolls within the current window. Resize clamps its position and scroll.
Escape, outside click and focus loss cancel without changing the document or
history. Input rewiring preserves both the original wire and the target's wire
until a successful atomic replacement, represented by one undo entry.

Preview and commit use the existing complete candidate connection checks:
types, semantic units, cycles, total connection budget and nested group budgets.
Explicit invalid socket targets do not fall back to another input on their card.
No compatible port, a collapsed node or an empty reverse drop gets a visible
reason. Normal user rejection and cancellation do not become fault logs.
Existing deliberate empty-canvas disconnection is preserved; dropping outside
the canvas cancels. No implicit casts, retries or source-format changes were added.

## Acceptance evidence

Evidence directory: `E:/Dev/aem/artifacts/node-wire-targeting`.

| Check | Final result |
| --- | --- |
| `cargo test -p motion-desktop --offline -- --test-threads=1` | 475 passed, 0 failed, 1 existing hardware-dependent test ignored |
| `cargo check -p motion-desktop --offline` | Passed; 21 existing compiler warnings |
| Python tooling unit tests | 54 run, 53 passed, 1 skipped |
| Panel style consumer guard and `git diff --check` | Passed |
| Default main window, zh/en | 1600 x 1000, both passed |
| Minimum main window, zh/en | 900 x 600, both passed |
| Native floating node window in all four cases | 1280 x 800; 32 checks per case, including 8 new wire checks, all passed |
| Native default log level and protocol isolation | `MOTION_LOG` unset; all four application stdout captures are empty |
| Repaint fixture | 1 render / 8 reuses, fixed Balanced tier; original Auto mode restored in all four reports |

Nine new pointer-driven regression tests cover nearest targets at 35/50/70/100/250%,
the visible high-zoom socket edge, front-card occlusion and clipped sockets;
unique body drops in both directions; ambiguous choice and Escape/outside/focus
cancellation; preserving both old edges until one atomic undoable replacement;
type, unit, cycle and budget causes; collapsed/empty drops; explicit invalid
ports with no body fallback; and a 64-output group chooser, long labels, wheel,
Home/End, mouse selection and resizing down to a 360 x 300 editor.

Native acceptance sends pointer/key input through the actual floating chrome,
then compares source connections and actual GPU pixel fingerprints. It verifies
deferred/cancelled choices, preservation of old edges on rejection, the chosen
port's rendered result, a nearest-B drop at 35%, editable source reopen and
restoration of the exact original document and GPU image through five undos.
The screen-geometry queries reject targets outside the current canvas, so an
off-screen source cannot silently turn a pointer test into a no-op.

`validation-summary.json` records all four native reports and source hashes;
`validation/` contains build/check/test logs, branch audit, initial failures and
the reviewed patch. The final executable SHA-256 is
`1ecb6bbb71c0ddc9132be0cf58ef39da797421fbc247c8f50c07befa23a9163b`.
Full-resolution native screenshots include `default/zh/node-smoke.wire-port-menu.png`,
`node-smoke.wire-replace-menu.png` and `node-smoke.wire-rejected.png`.
`review/` contains direct crops for readable menu review. The English minimum
window and Chinese minimum timeline dropdown captures were also visually checked.

Existing Epic Vulkan overlay JSON lookup errors and the U+2194 font fallback
observation still appear on stderr. They were not suppressed or reported as
new wire-targeting faults. No logging or protocol behavior was changed here.

The native suite exposed a separate acceptance-fixture race: Auto preview can
change its tier during the first composition render, changing the retained
stamp and requiring another render. A diagnostic run recorded 2 renders and 7
reuses instead of 1 and 8. The repaint fixture now temporarily uses Balanced,
checks that its tier stays fixed, and restores the original mode on success or
failure. Its existing 1-render/8-reuse assertion is preserved. Errors include
the actual counts; a queued event after a fatal result cannot overwrite it with
a success report. Primary failures survive restore failures. This changes the
acceptance setup, not production quality policy or caching. It does not prove
adaptive-quality behavior. The final default-level matrix must also verify
that the original Auto mode was restored.

The main integration also changed narrow timeline column allocation. Its
existing menu screenshot fixture still clicked the old blend slot after that
optional column had been hidden to reserve the name and 3D target. The fixture
now makes only the requested dropdown column visible and clicks that rightmost
column. It restores the original timeline state afterward. The assertion that
opening the menu emits no project action is unchanged. Production panel code,
column hiding rules and hit targets are untouched.

## Design and remaining limits

New chooser dimensions and states use existing `context.menu.width`,
`context.row.height`, `dropdown.max_width`, `padding`, `context_menu` and
`context_menu.row` styles. Body feedback uses existing semantic accent/error,
border, padding and row metrics. No style tokens, dependencies or assets were
added. The existing painted socket dimensions and legacy canvas wire/category
colors are retained, rather than claimed as fully tokenized.

No `desktop/app/src/panels/**` file was edited by A2/A3. All DESIGN.md §0.7 panel
remediation rows are retained outside this batch; inherited main changes are
not attributed to it. The node socket hit baseline is expanded, never shrunk.

A4 onboarding, A5 complete localized diagnostics/navigation and later audit
items remain pending. Long original diagnostic causes still use the existing
footer, with a short drag tooltip; this is not a full diagnostic drawer.
Group naming, reusable group instances, per-instance parameters, NodeSpec,
Gradient/Noise/UV Transform and example simplification are not delivered here.
This scoped completion does not close the overall node-editor usability goal.
