# Typed node input defaults acceptance

Status: B1 typed leaf defaults and scoped acceptance complete. Other audit items remain pending.

## Scope and coordination

- Audit item: B1, stored typed defaults edited on input socket rows.
- Branch: `feat/node-input-defaults`, stacked on the independently committed A1
  draft-validation change `4868eece80543f083b7e014dba522eb8b1d7081e`.
- Main base: `8d6698a506dee15b4b830d0a6904541ffb27ceba`.
- Worktrees, recent branches and open PRs were checked at
  `2026-10-18T10:35:43.241202+00:00`; no parallel input-default implementation
  was found. The read-only record is `.tools/node-input-branch-audit.json` in
  the primary checkout. Other dirty worktrees and their files were preserved.
- The local user-provided `desktop/NODE-UX-AUDIT.md` is a proposal. This batch
  does not claim to close that entire audit or deliver its geometry architecture.

## Changed behavior

`Node::input_defaults` stores exact Float, Vec2, Vec3, Color, Bool and
DistancePixels literals. A wire wins, including an incomplete wire through a
group boundary; removing it activates the saved fallback. Type, unit, value,
cycle, reachability and budget validation stay strict. Invalid defaults are
rejected even on unused or connected nodes. Colors store normalized straight
RGBA and follow the graph's existing alpha contract during compilation.

New desktop math, combine, select and numeric SDF inputs receive authoring
defaults. SDF position/image/distance sources remain explicit. Color Mix seeds
its factor; required color sources, including Invert's input, stay required.
Loading an old unfinished graph does not silently supply new values. Existing
examples and their explicit constant nodes remain unchanged. For a newly added
binary Math node, two literal inputs no longer require two separate constant
nodes. No aggregate 30–50% reduction in shipped examples is claimed.

Scalars use the existing drag/click-to-type editor. Vector components are edited
as comma-separated numbers on one input row. Bool uses a checkbox; Color uses
the existing picker. Wired fallbacks are disabled. New input fields share their
socket row; output rows follow inputs on cards with defaults to avoid overlap.
Legacy cards retain their layout. Style overrides cannot push the value field
outside the card. Existing socket hit tolerances are unchanged.

Committed gestures create one undo entry. Escape restores the starting value;
vector text owns navigation/deletion keys and does not commit during IME
composition. Operation changes retain compatible defaults, replace incompatible
types, and preserve the existing undoable connection-pruning behavior. The
inspector shows unconnected stored values on one read-only row per input.

## Source compatibility and boundaries

- Empty default maps are omitted; legacy `.msgraph` documents remain version 1.
- Documents containing defaults require version 2. Desktop edits upgrade the
  root automatically, including edits inside groups. Undo restores the previous
  document and version. Version 1 plus defaults and unsupported future versions
  are rejected explicitly.
- Internal group leaf defaults round-trip. Defaults on Group, GroupInput and
  GroupOutput boundary nodes remain unsupported and are rejected explicitly.
- Exported `.msfx` manifests and SDK 6 execution ABI are unchanged; the editable
  source retains the versioned defaults. Runtime rendering needs no editor.
- Context defaults, implicit casts, automatic example simplification, asset-like
  group parameters, UV Transform, Noise, Gradient and the other proposed
  primitives remain separate pending work. They are not marked delivered here.

## Verification

- `cargo test -p motion-graph --offline -- --test-threads=1`: **44 passed**,
  including nine real GPU tests. Seven new core tests cover format compatibility,
  every stored type, precedence/recovery, invalid data, nested groups, original
  missing-boundary causes and authoring defaults matching their declared ports.
- The new GPU test compares defaults against explicit constants at frames 0,
  15.5 and 29, in both straight and premultiplied alpha modes. The images must
  contain visible pixels and match byte-for-byte; exported source also reopens.
- Eight new desktop tests cover scaled dragging, vector validation/IME/key
  ownership, Bool/Color transactions, wired read-only controls, disconnect and
  undo, operation changes, nested save/navigation, row geometry and style bounds.
- `cargo check -p motion-desktop --offline`: passed with 22 existing warnings.
- `cargo test -p motion-desktop --offline -- --test-threads=1`: **444 passed,
  zero failed, one existing hardware-only ignore**. Existing assertions are unchanged.
- Final native matrix: **four cases passed**, each with all 24 node checks,
  six added in this batch. Every process kept stdout empty.
- Python tooling: **54 run, 53 passed, one existing skip**. Desktop style-consumer
  guard passed. Existing assertions were not changed.

The native fixture routes real pointer/text events through floating window
chrome and the normal Node Changed handler. It verifies live GPU changes,
wire precedence, disconnect recovery, scalar and vector editing, versioned
source reopening and eight-step undo restoration. CI requires its six new
boolean checks in addition to the eighteen existing node checks. An initial
fixture incorrectly rejected the normal Changed event; it was corrected to
route that event through the real shell handler, without weakening any checks.

Builds use this worktree's isolated `target` and a copied executable to avoid
cross-worktree build artifact reuse. Native output is retained under
`E:/Dev/aem/artifacts/node-input-defaults` (local evidence, not release assets).

## Visual scope and remaining limits

No `desktop/app/src/panels/**` file, style token, dependency, external asset,
composition schema or engine worker behavior is changed. All DESIGN §0.7 panel
baseline rows are **retained** because they are outside this node-input batch;
none is claimed fixed. New node-field rules are recorded in DESIGN and the
interaction reference. The implementation and examples are independently
authored; no third-party material is introduced.

The native matrix exercises Chinese/English at main-window sizes 1600×1000 and
900×600, with an actual 1280×800 floating node window. Unit input tests cover
0.5×, 1× and 2× canvas zoom. Other OS DPI configurations and physical mouse
trials are not claimed. No frame-rate, mobile performance or memory improvement
is claimed. The existing missing Epic Vulkan-overlay JSON and U+2194 font
fallback observations remain visible in stderr; this batch does not fix them.

## Final native evidence

| Case | Seconds | Checks | Stdout bytes |
| --- | ---: | ---: | ---: |
| default/zh | 79.58 | 24/24 | 0 |
| default/en | 77.81 | 24/24 | 0 |
| minimum/zh | 57.31 | 24/24 | 0 |
| minimum/en | 49.78 | 24/24 | 0 |

Final copied executable SHA-256: `f0c498eee7e37b961899f690cae769ed12a7b385f85e66ea5feb0fe88a317b95`.

Machine-readable results: `E:/Dev/aem/artifacts/node-input-defaults/validation-summary.json`.
Final build/check/test logs and the branch audit are retained in its `validation/` folder.
Native screenshots and source files are retained in each locale/size folder.

- Wired, disabled fallback: `default/zh/node-smoke.defaults-wired.png`
- Edited scalar/vector values: `default/zh/node-smoke.defaults-edited.png`
- One-row read-only inspector values: `default/zh/node-smoke.defaults-inspector.png`

A Windows linker attempt hit the test executable still running in an earlier
verification process. The final build, complete desktop test run and check
were serialized after that process exited. The final native matrix used the
copied executable with the hash above; no assertions or product behavior were
changed to bypass the file lock.
