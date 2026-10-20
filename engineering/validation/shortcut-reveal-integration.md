# Modified-property reveal and custom shortcut integration

## Problem and scope

PR #349 adds the double-press modified-property filter. PR #362 introduces
user-configurable bindings. Combining their unmodified heads leaves an
exhaustive command-label match incomplete and interrupts the reveal sequence
whenever its effective chord differs from physical unmodified U.

This compatibility change is prepared on #362 head `e2b491b`, which already
contains #357 and #368, and merges #349 head `d5a38bb`. It requires both #349 and
#357 before #362 is merged into main. It does not include the later layer
navigation, alignment, distribution or camera PRs. No new feature PR is opened.

Both `Panels` fields are retained. Animation commands resolve through the
effective user bindings; the same resolver identifies fresh/repeated reveal
presses. Existing context hashing, 350 ms sequence window, text/IME ownership,
focus and pointer interruptions remain intact. Both language labels cover the
new enum variant without inventing a new default shortcut or catalogue row.

## Regression evidence

The local all-PR integration first failed to compile with E0004 at the command
label match. After the label was supplied, the new routing test failed at the
second remapped chord: the modified filter was not selected. These are distinct
failures, not passing test counts.

The final regression exercises English/Chinese, Timeline/Properties/Curves/
Composition, unmodified Q and Ctrl+F4. It checks old binding removal, immediate
single press, double press, autorepeat interruption, a text owner's exclusive
input, and preserved undo/redo history. After text input, a fresh single press
must toggle an existing animated filter closed; a subsequent fresh press must
reveal modified properties. No existing assertion is changed.

In the separate all-PR integration, all 39 timeline shortcut tests passed after
this fix (`artifacts/pr-queue-timeline-shortcuts.log`). That tree also contains
later PRs, so this is historical combined evidence, not a fresh test result for
this smaller branch. The exact smaller branch at `907dc11` has now passed all
39 timeline shortcut tests (2.61 seconds), 12 modified-property tests (16.74
seconds), and seven binding/documentation tests (0.10 seconds). All commands
exited successfully. The full combined tree passed 1,196 cases, zero failures
and two existing ignores, including doctests; its code is unchanged by later
documentation-only head refreshes. That is combined evidence, not a new full
run of this smaller branch. Exact-head remote CI and native GUI acceptance
remain pending.

## Acceptance boundaries

DESIGN §0.7 property rows, effect lists and timeline row geometry are retained
with reason: this fix changes command routing and exhaustive labels only, with
no painting, style tokens, dimensions, control types or layout changes. Prior
PRs' actual screenshots remain their own evidence; they are not described as
new captures of this branch. DESIGN §12 final integration acceptance is pending.

No dependency, asset, copied UI text or Adobe binary is added. All work is in a
separate review worktree; other active branches are preserved. This compatibility
fix will be published to the existing #362 only after exact-head checks and
validation, not merged by substituting the all-PR integration branch.

## Native acceptance and downstream boundary

The first combined native GUI attempt used an unoptimized Debug binary. It
produced 50 real PNG captures before the existing Python 40-second deadline
expired. The failure log, default Info engine log and diagnostics snapshot are
retained under `artifacts/pr-queue-native`; it is not counted as a passing run.
The CI GUI check uses a Release binary. A fresh Release build is being prepared
with unchanged timeouts and assertions. Local Release acceptance does not
substitute for packaged FFmpeg or Mac verification.

A merge-tree preflight against #371 head `f13dd68` identifies a keyboard-router
conflict after this parent integration. The child must retain both configurable
UU routing and layer navigation. No child PR is merged on the strength of the
parent's tests. #349 and #357 must precede #362 on main.
