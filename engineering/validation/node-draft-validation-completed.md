# Node draft validation acceptance

Status: A1 implementation and scoped acceptance complete. Remaining audit items
are listed separately below.

## Scope and branch coordination

- Audit item: A1, dependency-based tolerance for incomplete graph drafts.
- Base: `origin/main` at `8d6698a506dee15b4b830d0a6904541ffb27ceba`.
- Branch: `fix/node-draft-validation`.
- Worktrees, recent branches and open PRs were checked at
  `2026-10-18T09:54:23Z` before implementation. PR #331 owns node grid/minimap
  visibility; this change does not modify its canvas rendering, menus or settings.
  Other dirty worktrees were preserved. The local audit record is
  `.tools/node-draft-branch-audit.json` in the primary checkout.
- `desktop/NODE-UX-AUDIT.md` is a user-provided local proposal, not a newly
  published capability specification. This change closes A1 only, not the audit.

Read-only `git merge-tree --write-tree` probes against #331 (`34d2083`) and #332
(`a089414`) auto-merged this patch's acceptance and CI files. They also reproduced
the same pre-existing conflicts as probes against unmodified `origin/main`:
`desktop/app/src/workspace.rs` for #331 and `desktop/app/src/panels/mod.rs` for
#332, with identical conflict blob IDs. Those files are not changed here. No
branches were actually merged, and combined runtime acceptance is not claimed.

## Behavior and limits

Previously, one supported but unfinished node anywhere on the canvas caused a
MissingInput failure, including during group flattening. Live compilation could
not update an otherwise complete output graph.

The compiler now checks mandatory inputs while traversing the selected output's
dependencies. Unused incomplete nodes, chains and nested groups remain in saved
source but are omitted from execution. Export includes the editable draft source;
the shader, manifest and public-parameter ABI remain identical when only an
unused draft is added.

An unfinished branch must be complete when connected to Color Output or explicitly
previewed. Deferred group-boundary failures retain their original qualified group
and port instead of becoming synthetic flattened leaf IDs. A genuinely incomplete
output still reports MissingInput and retains the last valid runtime.

Normal validation/compilation still checks full-document node identity/version, finite values and layout,
parameter metadata, connection endpoints, storage types, semantic units, duplicate
inputs, cycles, unsupported nodes and budgets. This is a functional change to
input-completeness validation, not a logging-only change. There are no implicit
defaults, conversions, retries, rewiring or source deletions.

## Evidence

| Check | Result |
| --- | --- |
| Regression on unchanged production code | First six draft tests all failed for the reproduced global MissingInput rejection or its masking of a cycle |
| Focused regression after implementation | 7 passed, including unused groups' structural rejection |
| `cargo test -p motion-graph` | 36 passed; includes 8 real GPU tests; independently rebuilt and repeated in this worktree's isolated target directory |
| GPU equivalence | Draft and original exports render identically at frames 0, 15.5 and 29; changing a public parameter with the draft present changes actual pixels |
| Saved/exported source | Source round-trip preserves unfinished nodes and wires; shader/manifest/parameter slots and sample count remain identical |
| Editor history | New regression covers unused partial wiring, a connected incomplete branch, exact diagnostic node/port, undo, redo and completion |
| `cargo check -p motion-desktop` | Passed; existing warnings retained |
| Desktop full suite | Initial Cargo run: 435 passed, 1 failed, 1 ignored; the same compiled test program passed in full with `--test-threads=1`: 436 passed, 0 failed, 1 existing audio-device ignore |
| Python tooling suite | 54 run: 53 passed, 1 existing skip |
| Formatting/style provenance | Changed compiler/test files pass rustfmt; changed native acceptance blocks formatted; `git diff --check` and panel style-consumer check pass |
| Native default/minimum, Chinese/English | All four isolated-build runs passed all 18 required checks; stdout remained empty in every run |

Local build/test logs are `.tools/node-draft-{before-shared,after,graph-tests,desktop-check,desktop-tests,desktop-build,python-tests}.log`
in the primary checkout. The initial redundant worktree-local build was stopped;
the recorded red/green evidence uses the shared target directory.

The first parallel desktop run failed
`panels::layer_actions::tests::double_click_name_only_opens_for_unlocked_layers`.
Its unchanged fixture creates a system-font atlas between clicks while production
requires a real-time interval below 400 ms. The unmodified base passed this test
alone (0.61 seconds for the entire test); the complete branch test program passed
serially (281.24 seconds). This is consistent with timing sensitivity under load,
not proof of a newly fixed product defect. No panel files or existing assertions
were changed. Logs: `.tools/node-draft-baseline-test.log` and
`.tools/node-draft-desktop-serial.log`. The preserved branch test executable is
`artifacts/node-draft-validation/desktop-tests.exe`, SHA-256
`59c93e8b6763325f9d1ea62e40d9ae9af72d1ff3beeef13eabeac638a798720b`.

After the base comparison, a shared-target native rebuild reused older dependency
artifacts and stalled at the draft acceptance step. That run is not accepted.
Final graph tests and native build use
`CARGO_TARGET_DIR=E:/Dev/aem/.worktrees/node-draft-validation/target` exclusively.
The first native pass before that comparison is retained as preliminary evidence;
final screenshot acceptance uses only the isolated build. Its native guard also
reports an unexpected draft diagnostic immediately instead of waiting for a
successful compile stamp until timeout.

Final logs: `.tools/node-draft-graph-isolated.log`,
`.tools/node-draft-build-isolated.log`, `.tools/node-draft-native-isolated.log`,
and `.tools/node-draft-python-final.log` (54 run, 53 passed, 1 existing skip).

Native acceptance adds an unfinished Invert node, waits for a fresh active compile
stamp, verifies unchanged GPU pixels, changes an active public parameter, verifies
changed pixels, then undoes it and checks restoration. The existing invalid-output,
grouping, ungrouping, project isolation, layer binding, undo/redo and offline-package
checks remain mandatory. `tools/ci_desktop.py` also requires the two new live-draft
checks. These observations distinguish a live new compile from stale retained
pixels.

Reproduce with a fresh binary using `--node-smoke <report.json> --locale zh` or
`en`; add `MOTION_SMOKE_MINIMUM=1` for the 900 by 600 main-window case. Floating
node windows retain their existing 1280 by 800 size. Native artifacts are under
`artifacts/node-draft-validation/{default,minimum}/{zh,en}/` in the primary checkout.
Both the accepted unused draft and a required-input error are captured as actual
native UI images. No context menu was changed.

The final executable SHA-256 is
`7ff8288867b8e9cc2c1622d115654f76917bfe13bc5aa708876161781dae8477`.
`artifacts/node-draft-validation/native-summary.json` records the same hash for
all four accepted runs (default Chinese/English: 53.67/48.30 seconds; minimum
Chinese/English: 46.84/48.83 seconds). These are acceptance-run durations, not
rendering performance measurements. The screenshot pair is
`node-smoke.unused-draft.png` and `node-smoke.required-input.png` in each case's
directory. The preview clock is paused in these captures for a stable frame.

The environment still emits the previously observed Epic Overlay Vulkan JSON
lookup errors and a missing `U+2194` glyph observation. They are retained in each
case's `stderr.log`; neither is fixed or suppressed by this patch. Their presence
does not stand in for a failed draft check or imply that stderr is empty.

## Design and remaining work

All DESIGN.md section 0.7 panel-remediation rows are retained: this patch changes
no panel production code, styles, tokens, row geometry or hit regions. It does not
claim that the broader remediation baseline is compliant. Screenshot acceptance
is scoped to node draft behavior; high-DPI coverage is pending. No performance or
mobile-portability claim follows from single-pass shader equivalence.

The remaining node optimization work includes input defaults, UV Transform,
Map Range and vector splitting, Gradient, color ramps, blend/HSV operations,
budgeted noise, search/wiring feedback and reusable group instances. These are
pending, not implemented by this change. Existing group renaming and whole-graph
layer effects must be considered before introducing a new packaging mechanism.

No dependencies, third-party assets, Adobe resources, project format changes or
new UI controls are introduced.
