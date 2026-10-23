# Distinguish layer deletion and preserve floating error priority

Related #158; the broader shortcut issue remains open. Baseline main is
`3d18f05f`. Before implementation, the audit checked 366 head/remote references,
50 distinct keyboard/localization sources, and 107 worktrees. No layer deletion
target feedback was found. Dirty historical integration/locking branches had
no equivalent feedback changes and were left untouched.
The additional floating-error audit inspected 38 distinct panel sources across
366 references and found that priority rule only in this new branch. Before
publication the unchanged patch was replayed onto main `62bf0f28`; range-diff
reports identical patches. The original keyboard route body was compared
directly and remains unchanged.

## Functional changes

Keyboard actions that actually request `Command::Delete` for a layer now publish
the localized status `删除目标：图层` / `Delete target: layers`. This identifies
the requested target; it does not claim backend success. Single and batch
actions, including composition wrappers, use the existing commands.

The public keyboard route retains its signature, consumption result, handlers,
ordering, and original body. A wrapper examines only actions appended by that
call. It does not mutate the actions. No new actions leave existing messages
alone. A subsequent non-layer action clears only this feature's own exact
En/Zh notice, so a keyframe deletion cannot retain a layer-target notice. Other
messages are retained. An actual new layer operation replaces the old status
with its new target notice. Empty/locked selections, IME/text ownership,
unsupported chords, and OS repeat remain normal branches.

Separately, the floating status bar now gives an existing nonempty
`renderError` precedence over drag readouts and ordinary messages, matching the
main window's existing rule. It uses the existing `error` style. Healthy
floating status text keeps `label.small.muted` and the exact original rectangle.
No new error structure, style, color, dimension, row, or hit region is added.

## Failure evidence and verification

The first four tests ran against original production logic: **1 passed,
3 failed, 0 ignored, exit 101** (`original-feedback-tests.log`). They observed
missing target feedback, Ready glyphs instead of the target, and a retained
old layer notice after key deletion. Blocked inputs and an existing action
prefix already passed.

The first feedback implementation passed those four tests. A six-test phase
then found floating error precedence missing (**5 passed, 1 failed** in
`error-priority-regression.log`). That phase combines a real rejected core
delete with a deliberately supplied error snapshot; it does not prove host
publication by itself.

The next phase added an actual host failure path: real `DesktopPlatform`, real
Session, and real UI-generated delete commands. A controlled invalid sample
time is set before applying the edit. The edit commits, subsequent sampling
fails, and production `apply_command_batch` returns `Ok(snapshot)` carrying
the original `renderError`. The session is closed before paint assertions.
Both error probes fail at the floating status (**5 passed, 2 failed**, in
`host-error-priority-regression.log`); main-window error paint already passes.
Original nonempty/exact glyph and model assertions are preserved.

Final checks on the current main baseline passed **25 keyboard**, **48 timeline
shortcut**, and **13 chrome** tests: **86 distinct tests**, 0 failed, 0 ignored,
including seven new tests. Logs are `main-keyboard-tests.log`,
`main-shortcut-tests.log`, and `main-chrome-tests.log`. An initial adjacent-suite
filter matched zero tests; `empty-filter-excluded.log` preserves that run and it
is excluded from acceptance. The actual test module was then run with a
nonempty-suite guard. Intermediate phases are not added again.

Product `cargo check` passed with 19 existing warnings (`check.log`). New-file
rustfmt, provenance, panel style-consumer, and diff checks passed. Exact-head CI
and merge remain pending.

New test scope includes 32 locale/focus/owner/selection/key combinations, actual
single/multi core deletion, unselected layers retained, save/load, and one-step
undo. Real chrome/floating_chrome draw the target at 900×600 and 1600×1000 and
retain it after the deleted selection refreshes. Additional probes distinguish
key deletion, verify remapping and Undo notice retirement, preserve unrelated
messages, and reject false attribution from pre-existing action prefixes.

## Observation and protocol audit

| ID | Classification and disposition |
|---|---|
| LD-TARGET | New UI feedback for the actual emitted layer-delete operation; no success claim. |
| LD-LIFETIME | New ownership-limited retirement of this notice on subsequent non-layer actions, across both message languages. |
| LD-FLOAT | Functional error presentation fix; original nonempty renderError takes precedence in floats. |
| LD-CORE | Existing boundary: a stale delete returns its real Err without model/revision mutation. |
| LD-HOST | Existing degraded-success boundary: committed host edit with failed sampling returns Ok(snapshot)+renderError; the edit and cause are retained. |
| LD-PERSIST | Existing command/history/storage behavior; actual persistence and undo probes verify it. |
| LD-NORMAL | No-action, cancellation, ownership, missing selection, and repeat suppression are normal, not new faults. |
| LD-OBSERVE | No new logging, stdout, counters, file IO, worker, timeout, retry, restart, or panic suppression in product code. |
| LD-PENDING | Physical OS windows/DPI, serial-worker failure transport, and default file/Error-stderr observation acceptance are not newly proven by these UI/core/host tests. Existing observations remain unchanged. |

The real host fixture uses controlled fault injection in public session state;
it does not claim that the UI permits an invalid sample time. The separate core
rejection and manual snapshot test does not stand in for this real publication
path. No error is cleared to obtain acceptance, and no existing assertions or
test ignore gates are relaxed.

## DESIGN §0.7 and §12; provenance

PROVENANCE.md, ATTRIBUTION.md, NOTICE, and DESIGN.md were checked. Text and code
are independently authored. Fixtures use existing core/host/platform APIs and
system UI fonts; no font files, icons, external text/assets, or dependencies are
added. There is no persistent project schema or keyboard mapping change.

| DESIGN §0.7 baseline | Disposition |
|---|---|
| Path six literal row steps | retained with reason: path rows untouched. |
| Path two-column split | retained with reason: columns untouched. |
| Path button/field heights | retained with reason: controls untouched. |
| Path numeric background | retained with reason: numeric editing untouched. |
| Path boolean wide button | retained with reason: boolean editing untouched. |
| Effect row heights/steps | retained with reason: parameter rows untouched. |
| Effect boolean buttons | retained with reason: boolean controls untouched. |
| Effect enum wide buttons | retained with reason: enum controls untouched. |
| Effect hover alpha | retained with reason: effect hover states untouched. |
| Effect indentation/hierarchy | retained with reason: property tree untouched. |
| Timeline label coordinates | retained with reason: compatibility baseline preserved. |
| Timeline snap color | retained with reason: existing drag feedback preserved. |

DESIGN §12 acceptance is limited to the status-message/error drawing and the
verified input, model, persistence, and history behavior. Compilation alone is
not acceptance; all-panel remediation, physical-window/DPI and GPU screenshots
remain pending.
