# Construct layer-lock batches only for the Layer menu

Related #134 (UI hot-path allocations); the broader issue remains open.
Baseline main: `afd29f5a`. The branch audit checked 360 head/remote references,
22 distinct menu sources, and 106 worktrees before implementation. No current
`selection(state, panels)` menu source deferred these batches. A historical
dirty `layer-locking` worktree used an older toggle/unlock API and dated from
October 18; its original feature is already superseded on main. It was retained
untouched. Current main's unconditional work traces to `817afdb05`.

Every open menu previously constructed selected-layer lock commands and an
unlock-all batch, even when those actions belonged exclusively to the Layer
menu. Each locked layer can allocate both its composition string and boxed
command on every redraw. The change moves the same construction into menu 3's
existing match arm, with the same state and selection each frame. No cached
commands, stale-selection behavior, new transaction, or logging are introduced.

## Evidence and limits

The original production renderer failed the allocation-growth probe (0 passed,
1 failed, exit 101), retained in `original-allocations.log`. With only the four
construction statements moved, the identical probe passed (1 passed, 0 failed,
0 ignored), retained in `final-allocations.log`. No assertion was weakened.

For 20 warmed English File-menu redraws:

| Locked layers | Before allocation requests / requested bytes | After requests / bytes |
|---|---:|---:|
| 1 | 1,680 / 1,654,940 | 1,620 / 1,558,060 |
| 512 | 22,260 / 31,258,540 | 1,620 / 1,558,060 |

All eight non-Layer menus in both En/Zh (16 cases) now have identical request
counts and bytes at 1 versus 512 locked layers. Both Layer-menu cases retain
their complete original measurements at both sizes, including 23,240 requests
and 32,951,820 bytes for English/512 layers. The allocator canary detects a
real String allocation. `allocation-receipt.json` checks all 18 before/after
records, not only the File example. Fixtures are UI snapshots with layer IDs,
visibility, lock flags, and composition identity; this is actual menu rendering,
not a claim of complete persisted-project or physical-window acceptance.

After removing all instrumentation, the ordinary menu suite passed **33 tests,
0 failed, 0 ignored** (`final-menu-tests.log`), including real lock/unlock batches,
preserved visibility, composition scope, save/reopen, one-step undo, custom
shortcut hints, main/floating routes, and focus/cancellation cases. These are
existing regressions, not 33 new tests. Product `cargo check` passed with 19
existing warnings (`check.log`). Provenance, panel style-consumer, and diff
checks passed. Exact-head CI and merge remain pending.

Allocation probes use the actual production menu renderer with a temporary,
thread-local counting allocator and real UI text atlas. They count successful
allocation/reallocation requests and requested bytes; this is neither peak live
memory nor GPU memory. Fixtures and font warmup happen outside the measurement.
The probe and its absolute local test-module includes were removed after the
comparison; their source/logs are retained under
`.tools/menu-layer-lock-laziness` for the before/after experiment. The production
patch does not install an allocator or add a test instrumentation dependency.

Normal disabled menus, cancellation, and empty selections remain normal. Existing
core/shell command errors, rollback, undo, and observations keep their existing
boundaries. There are no new observations, stdout writes, file IO, timeout,
retry, restart, panic handling, or changed return values. Do not interpret healthy
menu checks as new logging/default-stderr acceptance.

| Audit ID | Classification / disposition |
|---|---|
| ML-PERF | New performance correction: only the Layer menu constructs lock/unlock batches; actual allocation growth removed in 16 cases. |
| ML-LAYER | Existing functional boundary: Layer menu retains fresh per-frame commands and identical measured work. |
| ML-NORMAL | Empty/disabled/cancelled UI branches remain normal; no new faults/counters. |
| ML-OBSERVE | Existing observations retained, no new logging or protocol behavior. |
| ML-PENDING | Remaining #134 work, whole-frame/FPS claims, physical-window/DPI visuals, and CI/merge remain outside current completed evidence. |

The generic editable-selection lookup, text, paint buffers, and Layer menu's
own work remain. The evidence does not claim zero total menu allocations,
all-layer scans removed, FPS improvement, or a fix for every #134 item.

## DESIGN and provenance disposition

PROVENANCE.md, ATTRIBUTION.md, NOTICE, and DESIGN.md were checked. No third-party
assets, dependencies, copied text, compatibility names, token changes, colors,
dimensions, control types, menu geometry, or hit tolerances are introduced.

| DESIGN §0.7 baseline | Disposition |
|---|---|
| Path six row steps | retained with reason: path rows untouched. |
| Path two-column sizing | retained with reason: column layout untouched. |
| Path button/field heights | retained with reason: controls untouched. |
| Path numeric background | retained with reason: numeric editing untouched. |
| Path boolean full-width button | retained with reason: boolean controls untouched. |
| Effect row heights/steps | retained with reason: parameter rows untouched. |
| Effect boolean buttons | retained with reason: controls untouched. |
| Effect enum wide buttons | retained with reason: enums untouched. |
| Effect hover alpha | retained with reason: hover rendering untouched. |
| Effect indentation/hierarchy | retained with reason: property tree untouched. |
| Timeline label gutter coordinates | retained with reason: preserve compatibility baseline. |
| Timeline snap color | retained with reason: preserve existing drag feedback. |

DESIGN §12 acceptance is limited to the verified menu behavior and allocation
scope. Physical window/DPI screenshots, all panels, and global remediation are
pending rather than implied by compilation or moving code.
