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

## Squashed preference-parent integration

PR #357 is now squash-merged on main at `8bf0874b`, after #368 and #367.
Git's default historical merge base caused twelve apparent add/add or field
conflicts when integrating that main into this branch. The original #357
head `e19a49b2` is an ancestor of this branch, and the exact source difference
from that parent to current main is only the six already merged grapheme files.

A read-only merge using that explicit logical base was clean, yielding tree
`a13e84b290a1512f4bc53d5b813ec93014c8233f`. The actual merge restored only the
twelve conflicted paths from that computed tree and required `git write-tree`
to match it exactly before committing. The resulting first-parent diff is
exactly the six grapheme files; shortcut routing, preference behavior and
property-reveal source were not changed. Whitespace checks passed.

This is local integration evidence, not a fresh Rust run or remote CI result.
The integration is held locally until #349 also reaches main, avoiding another
CI restart merely for ancestor history. The full combined Release GUI suite
has since passed with its original deadlines and 1,177 decoded PNGs; this is
separate combined evidence and does not replace the final branch's checks.

## Shared CI correction and current-main tools

The local candidate integrates #374 `e57e84ce` with a clean merge matching
preview tree `541828c0`. Six incoming files match that parent exactly; main.rs
only adds the Windows test-helper declaration while retaining the existing
preference and shortcut registration. The CI routes, native helper, fixture
deadlines and assertions retain their independently reviewed behavior.

After #353 reached main as `3dd45748`, its integration was also clean and
matched preview tree `4e61560e`. The only four incoming files are the icon tool,
its regression, tool inventory and validation document. No shortcut or panel
source was altered by either integration.

The resulting local candidate `8addf61` completed all 57 tool tests in 39.438
seconds: 56 passed and one existing optional external-loader check skipped.
Provenance (1,557 tracked paths), panel style-consumer and whitespace guards
passed. Evidence: `artifacts/pr-queue/362-current-main-tools.log`. These checks
include the combined trigger policy and icon failure regressions. They are
not a new Rust or native GUI run. This candidate is still local, and #349's
final Mac check and main integration remain prerequisites to publication.

## Actual main integration after #349 merged

#349 passed all six checks and was squash merged as `a3ca3bd9` after the
independent #370 merge. Candidate `3ab84dc0` integrates this real main. Only
`keyboard.rs` and `types.rs` conflicted; their resolutions retain the previously
tested candidate's complete bytes, including effective user bindings, immediate
U, repeated UU, repeat interruption and text/IME ownership. The final tree
`03c95f71` differs from tested candidate `57ff99cb` only in the three incoming
#370 renderer/test/document files. No shortcut, preference, menu, layout or
style source changed during this integration.

Fresh current-source tests: timeline shortcuts 39 passed (1.73 seconds), modified
property classification 11 (0.15), bindings/documentation 7 (0.09), native message
pump control 1 (0.27), bilingual PNG export 1 (12.07), and four-mode sampling 1
(22.38): 60 unique cases, no failures or ignores. The sampling artifact run
retains five warmups and sixty strictly increasing finite frames in each mode;
the root and per-mode records match. An earlier sampling invocation passed in
25.85 seconds but did not retain its output because the artifact environment
variable was mistyped. It is not counted as an additional unique case.

All twelve native PNGs were fully decoded. Both retained logs were generated
without MOTION_LOG and contain Info start records with no Present start records.
Provenance (1,559 tracked paths), style-consumer and whitespace guards passed.
Evidence: `.tools/issue-closure-review/362-actual-main-{integration,acceptance,
png-verification}.json` and `artifacts/pr-queue/362-actual-main-*`. Historical
full-suite and UI screenshots above retain their original source scope; this
is a targeted new-source run, not a complete desktop rerun. The following
documentation-only commit does not change the tested source. Current-head
remote CI, high DPI and other-platform manual interactions remain pending.
