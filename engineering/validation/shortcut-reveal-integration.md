# Modified-property reveal and custom shortcut integration

## Problem and scope

PR #349 adds the double-press modified-property filter. PR #362 introduces
user-configurable bindings. Combining their unmodified heads leaves an
exhaustive command-label match incomplete and interrupts the reveal sequence
whenever its effective chord differs from physical unmodified U.

This compatibility change is prepared on #362 head `e2b491b`, which already
contains #357 and #368, and merges #349 head `63e56d9`. It requires both #349 and
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
this smaller branch. The exact branch's targeted tests, full desktop regression,
native screenshots and final remote CI remain pending.

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
