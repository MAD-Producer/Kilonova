# Repository agent rules

This repository follows a clean-room implementation policy.

## Prohibited

1. Do not decompile, disassemble, extract, or inspect Adobe After Effects, Photoshop, Premiere, or other Adobe binary content, including string tables and resource sections.
2. Do not copy Adobe icons, pointers, fonts, audio, presets (`.ffx`), templates, screenshots, or text from product documentation into the product.
3. Do not use Adobe trademarks or product logos in the Motion Studio product name, logo, icons, UI, package identifiers, or release artwork.
4. Do not add third-party assets without recording the author, source, revision, license, and modification scope in `ATTRIBUTION.md`.

## Permitted references

Public Adobe user guides, public scripting/API references, public shortcut references, standards, papers, and community discussions may be used to understand documented behavior. The implementation, assets, and product language must remain independently authored.

Before adding a dependency, asset, compatibility name, or copied-looking UI detail, check `PROVENANCE.md`, `ATTRIBUTION.md`, and `NOTICE`. Flag any violation directly in the pull request.

## Visual style

`DESIGN.md` is the single source of truth for UI presentation. `desktop/ui/styles/dark.json` carries the token values; `DESIGN.md` carries the rules, the remediation baseline, and the acceptance checklist. When they disagree, `dark.json` wins on values and `DESIGN.md` wins on rules.

Never write a literal color or dimension in panel code — reference a semantic token. `tools/ci_desktop.py` fails the build on `palette::` or `default_stylesheet(` in `desktop/app/src/panels`. `DESIGN.md` §2 forbids inventing `accent-strong`, `playhead`, or `layer-label-*` style names that have no existing token. Any token change must be written to `dark.json` first, never in paint code.

Property trees and effect lists have a hard row contract, `DESIGN.md` §5.2. One property per row, all controls inline, every dimension read from a token. Never render a switch or an enum as a `ui::button`, never cycle enum values on click, never open a value field's background unless it is being edited, and never add a row to make room for a control. Do not shrink existing hit tolerances to gain density. Those are the most common regressions in `desktop/app/src/panels`.

`DESIGN.md` §0.7 is the remediation baseline: every row is a known existing violation with a file and line. Check it before editing a panel, and report each row as fixed / retained with reason / pending verification. `DESIGN.md` §12 is the acceptance checklist; compilation success is not acceptance.

## Error handling and observability

Account for `Err`, panics, operations that never return, and degraded paths that return `Ok(false)` or `Ok(None)`. Logging only `Err` branches is insufficient. Normal cache misses, cancellation, superseded requests, and empty undo stacks are not faults.

- Use the existing `log` infrastructure. Failures must be diagnosable at the default `MOTION_LOG=info` level. Write Error records to both the log file and console stderr; write other levels to the file. Never log to stdout: command listings and MCP JSON-RPC use that channel
- Include available request IDs, operations, object/source IDs, phases, elapsed time, and original causes. Preserve the primary failure when cleanup, rollback, or closure also fails. Do not log complete user text, credentials, or unbounded payloads
- Publish in-flight state and log Info start for non-Present commands before a serial worker enters a handler, decoder, or another potentially blocking operation. Record completion timing and clear in-flight state afterward. Completion records cannot replace the start signal
- Never log start/done/dispatch unconditionally on the Present hot path. Keep healthy Present calls quiet and log slow calls at Debug using the existing threshold. Use Info for non-Present start, Debug for routine done/dispatch/timing, and Warn/Error for sustained degradation or actual faults
- Bound counters and rate-limit frequent silent degradation. For consecutive failures, normally log Debug on the first occurrence and Error once at occurrence 60, then reset on success or a normal miss. If a counter accumulates over an instance's lifetime instead, document its scope and reset conditions. Do not equate 60 occurrences with one second
- Bound observation keys, retained causes, and context lengths. Include useful context such as paths, code points, and surface states. Healthy hot paths must not add file IO or per-frame warnings for normal polling
- Panic hooks must use `Backtrace::force_capture()`, avoid `unwrap`, and access the logging sink with `try_lock`. Fall back directly to raw stderr if unavailable, then preserve the original hook. Do not call the logger while holding business-state or observation locks
- Protocol-defined best-effort replies may be discarded after their receiver exits. Preserve `respond` semantics. Do not hide failures by clearing errors, swallowing panics, changing return values, or adding retries or restarts
- Preserve functionality and interaction behavior when adding observations. Explain functional fixes, thread timeouts/restarts, UI error structures, layout, or style changes separately. These logging rules do not waive the panel and design constraints above
- Verify actual failure paths and output at the default level, including file logs, Error stderr, and stdout isolation. Test thresholds, suppression, and recovery resets where applicable; use controlled blocking to verify state publication before the handler. Compilation, green test counts, or mocked successful returns alone do not prove observability
- Audit records must distinguish new observations, existing coverage, normal branches, functional/protocol boundaries, and pending work. Record item IDs, evidence, and remaining limits in the completion document. A documented disposition is not necessarily a fixed defect. Never change existing assertions merely to pass acceptance
