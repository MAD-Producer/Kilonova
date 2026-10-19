# Node naming acceptance

Scope: node-editor audit item A7. This branch starts from main `80da408`.
PR #337 contains the preceding primitives and examples; PR #338 contains
catalogue search. Neither those PRs nor this record close the complete audit.

Status: local acceptance complete. The final binary passes the full desktop
acceptance script and all four native naming profiles. Remote CI is evaluated
separately on the PR.

## Behavior and evidence

| Requirement | Implementation and verification |
| --- | --- |
| Rename ordinary nodes | A native context-menu command edits optional `Node.display_name`; canvas and inspector resolve the alias before the operation title; clearing it restores the default |
| Name groups | The same command updates existing group metadata, including the name inside the child graph; defaults are numbered in both languages |
| Rename ports | A separate context submenu lists group inputs/outputs; edits update the appropriate boundary metadata, including from within a group |
| Preserve identities | Node IDs, parameter IDs, socket keys, types, semantics and connections stay unchanged; comparisons cover WGSL, preview stamps, undo and redo |
| Readable parameters | New public parameters receive numbered localized labels by type; stable machine IDs and existing bilingual parameter-inspector fields remain intact |
| Readable exposed ports | Automatically exposed port labels use the node's visible title and port description; socket keys remain machine identifiers |
| Safe text transactions | Native selection/caret editing, IME confirmation, Enter/Apply, Escape, focus loss and outside-click cancellation; opening or cancelling creates no history entry |
| Bounded names | At most 120 UTF-8 bytes of visible text; API rejects control-only, blank and oversized names as applicable; oversized paste is rejected rather than silently saved as a shortened alias |
| Persist and reopen | Optional aliases roundtrip; native probes save actual `.msgraph` files and load them again; ordinary empty aliases are omitted, preserving legacy JSON |
| Native presentation | Existing semantic colors and new `node.rename.*` dimensions in `dark.json`; no panel files changed |

The native probe uses a disposable, genuinely compiled and installed editor.
It drives the actual right-click menus and native text input in docked and
floating windows, then restores the original editor and GPU preview, including
when the primary probe fails. It rejects unexpected graph/project actions.

## Verification

- `cargo check -p motion-desktop --locked`: passed
- `cargo test -p motion-desktop naming_tests --locked`: six new tests passed
- `cargo test -p motion-graph --test naming --locked`: two new tests passed
- `cargo test -p motion-desktop node_editor --locked -- --test-threads=1`: 73 passed
- `cargo test -p motion-desktop --locked`: 533 passed, no failures, one existing ignored device test
- `cargo test -p motion-graph --tests --locked -- --test-threads=1`: 30 passed across ten suites, including GPU coverage
- `cargo test -p motion-ui --test style --locked`: six passed
- Native `--node-smoke`: English/Chinese, default and minimum windows all passed, with 48 naming checks and 56 named screenshots; all four stdout files are empty
- `tools/ci_desktop.py`: passed on the final binary, including both languages, MCP, property panels and style overrides
- Provenance guard and whitespace checks: passed before staging, repeated before commit
- Pairwise `git merge-tree` checks with PR #337 and PR #338: both passed without text conflicts

The complete desktop unit run was repeated after the final acceptance-helper
isolation and viewport-fixture adjustment. Native profiles use the final binary.
`artifacts/node-naming/native-summary.json`
records its hash, report paths, image dimensions and per-profile checks.
Compiler and unit logs are retained as `artifacts/node-naming-*.log`.

## Boundaries and remaining work

- Existing graphs without `display_name` load unchanged. Older binaries with
  strict unknown-field rejection cannot load a graph with a nonempty alias;
  this forward-compatibility limitation is documented in the graph README
- An authored group or port alias applies to both localized name fields;
  the existing group inspector still allows bilingual group names separately
- Reusable group definitions, independent instance parameters and bulk port
  exposure (B5) are not implemented by naming; gesture work in A8 remains open
- Minimum-window testing reproduced an existing Home/frame-all issue: a
  canvas around 81 pixels high loses almost all usable height to fixed
  80-pixel fitting padding, making a complex graph overlap at near-zero zoom
  The naming probe pans a readable target into view before testing real input;
  it does not claim to fix frame-all. Evidence is retained in the initial
  minimum-window naming captures and the initial failed native-run log
- The narrow-timeline menu acceptance fixture correction is identical to
  PR #338/#337: expose the requested optional column and click its visible
  target, preserving all assertions and product behavior
- All DESIGN.md section 0.7 panel baseline entries are retained: this branch
  changes no panel implementation. They remain outside this naming batch
- Remote checks must be evaluated on this branch's PR; local results are
  not a claim that the complete node-editor goal or all visual rules are met
