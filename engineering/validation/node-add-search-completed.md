# Node catalogue search acceptance

Scope: audit item A6, catalogue discovery in the native Add popup and node
library. This is a separate branch based on main `80da408`; PR #337 contains
the preceding node primitives, draft editing and compact-example work.
This document does not declare the whole node-editor audit complete.

Status: local acceptance complete. Final code passes all 540 desktop tests
and four native profiles. Remote CI is evaluated separately on the PR.

## Behavior and evidence

| Requirement | Implementation and verification |
| --- | --- |
| Search across categories | One matcher serves the popup and library; nonblank queries ignore the browsing category, blank queries preserve it; real input tests search Circle from Math |
| Find familiar names | Independently authored English/Chinese aliases, pinyin and initials; all whitespace-separated query tokens must match; title matches rank first with stable catalogue order |
| Explain results | A title and description per result, plus actual input/output names and types from graph port metadata; public parameters expose the selected type |
| Edit and select safely | Native TextEditor caret, selection, Home/End, Delete/Backspace and Unicode handling; queries retain at most 128 characters; Up/Down and Enter select a result; IME confirmation never adds |
| Preserve placement and cancellation | Popup retains its opening world position; empty results, Esc, outside clicks and rejected additions leave source unchanged; failed additions do not update recents |
| Keep recent choices | Only successful catalogue choices update 16 deduplicated keys; saved keys are limited to 96 bytes and checked against the catalogue; unknown and repeated keys are ignored |
| Keep UI state outside graph history | Workspace persistence survives reopen and legacy JSON; undo, graph replacement and switching between draft/layer editors retain recent choices; source format is unchanged |
| Work in short docks | Popup removes its redundant scope heading, retains a full result and port footer, and scrolls categories independently; short libraries retain the search field/results before optional headings/details |

`search.rs` contains catalogue metadata, ranking and recent-key validation.
`search_input.rs` wraps the existing native editor. Shared result/signature
painting lives in `view/search.rs`; the popup and library use the same data.
`node_search_acceptance.rs` drives real native input and captures the GPU UI
in both docked and floating windows. Its temporary editor is really compiled
and installed; the original editor, preview and workspace recents are restored.

Thirteen added tests cover aliases/ranking, every query token, unique metadata,
bounded/deduplicated persistence, native text editing and IME, successful and
failed choices, source/undo invariants, geometry, taller theme rows without cropped descriptions, and reaching the last category
in a short popup. Existing assertions remain unchanged.

## Validation

- `cargo check -p motion-desktop --locked`: passed
- `cargo test -p motion-desktop node_editor --locked -- --test-threads=1`: 80 passed
- `cargo test -p motion-desktop --locked -- --test-threads=1`: 540 passed, no failures, one pre-existing ignored device test
- `cargo test -p motion-ui --test style --locked`: six passed
- `python tools/validate_provenance.py`: passed
- `git diff --check`: passed
- `tools/ci_desktop.py`: passed, including both languages, MCP and styled floating/property windows, before the final one-line result-text clipping fix; the full 540-test desktop suite was rerun after that fix and the final binary passed four native profiles (English/Chinese, default 1600x1000 and minimum 900x600 main windows): 48 search checks and 48 captures, all stdout logs empty

Local logs and native captures are retained under
`artifacts/node-add-search/`, with compiler/test logs retained alongside the reports. The final `native-summary.json` records the binary hash, report
paths, individual search screenshots, dimensions and stdout checks.
Native tests use the existing hidden-window entry point and actual GPU paint
and input handling; screenshots are not mockups.

Reproduce from the repository using a built desktop binary:

```powershell
python tools/ci_desktop.py --binary target/debug/motion-studio.exe --output artifacts/node-search
$env:MOTION_SMOKE_MINIMUM = '1'
target/debug/motion-studio.exe --node-smoke artifacts/node-search/minimum-en.json --locale en
target/debug/motion-studio.exe --node-smoke artifacts/node-search/minimum-zh.json --locale zh
Remove-Item Env:MOTION_SMOKE_MINIMUM
```

## Boundaries and provenance

The short-window timeline-menu fixture correction is identical to the one
already validated in PR #337: expose the requested optional column and click
its visible location. It fixes the fixture's hidden-column target, retains
all existing assertions, and does not change timeline production code.

No panels files, graph schema, dependencies or third-party assets change.
`DESIGN.md` section 10.2 and `dark.json` record the new presentation contract.
Section 0.7 panel remediation is outside this batch and is not claimed fixed.
Existing unrelated node-canvas literals are not globally restyled.

Recents are catalogue choices, not reusable user graph assets. Saving group
assets, instance-specific parameters, automatic conversion/insertion and
NodeSpec remain separate audit work. The shell's existing Esc focus behavior
is retained; the native smoke explicitly re-focuses before its next independent
search probe. The rest of the audit must be rechecked against main and #337.

On this machine the Vulkan loader reports missing Epic overlay manifests to
stderr. These original third-party Error records are preserved. They are not
suppressed or used to substitute an unsuccessful native exit with success.
