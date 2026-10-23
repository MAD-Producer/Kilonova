# Effect menu removal scope (#265 subset)

This change implements only the explicit removal scope and shared removal action
portion of #265. Recent/favorite application, preset saving (#225), and the rest
of #265 remain open. It reuses `Command::Effect(Remove)` and `Action::EditBatch`;
there is no new command, transaction, worker, logging protocol, or dependency.

Before implementation, the branch audit inspected 349 references and 132
worktrees. No Effect-menu removal implementation was found. One historical dirty
worktree contained an existing context-header locking change already present in
the baseline; it was left untouched. The dedicated branch initially used the
font fix from #458 so Chinese glyph acceptance could use the corrected picker.
After #458 merged as `f50c550c`, this patch was rebased onto main `c507846f`;
the font changes are not part of this PR. Evidence stays under `.tools/effect-menu-remove-all` in the
shared workspace; it is not shipped with the product.

## Scope and boundaries

- The Effect menu removes every effect from the currently selected layers. Its
  enabled caption identifies the number of selected layers and effects.
- The effect context menu keeps its existing single-layer scope and names it.
- Both hosts share eligibility and command construction. Read-only, locked,
  missing, malformed, and empty targets are unavailable; a multi-selection is
  never filtered into a partial editable subset.
- Cancellation and unavailable selection are normal UI branches, not faults.
  Engine errors remain errors; an invalidated batch uses the existing rollback
  and return semantics. No retry, swallowed panic, cleared error, or changed
  result is introduced.
- Existing top-level chrome Escape cancellation and context-menu
  Escape/focus/outside cancellation are retained and exercised through their
  real input owners.
- Existing shell batch handling and logging remain the observation boundary.
  No new observations or stdout output are added. Functional tests do not prove
  new default-level log/stderr coverage, and none is claimed.

## Verification record

The original production implementation failed the new real-menu probe: clicking
the expected removal row emitted no action (0 passed, 1 failed, exit 101).
`original-regression.log` retains the failure. No existing assertion was changed
or test ignored to obtain acceptance.

The first implementation passed seven focused tests. After adding the complete
host-routing probe, the final controls suite passed 32 tests, and the neighboring
menu suite passed 37 tests, all with zero failures and zero ignored tests. Four
test names overlap: the union is **65 distinct tests**, including eight new tests;
the intermediate seven are not added again. Logs are `final-controls-tests.log`
and `menu-regressions.log`.

Evidence includes actual menu-generated core commands, all selected effects
removed in one batch, unselected effects retained, actual storage save/load,
and one undo restoring the entire project. A previously generated batch is
invalidated by a real core effect removal; applying it returns Err and retains
the pre-batch model, revision, and undo/redo state, including the earlier valid
undo entry. This verifies existing atomic behavior, not a new rollback fix.

Both locales and both 900×600/1600×1000 sizes exercise real `chrome` and
`floating_chrome` effect-context input routing (eight configurations). These
verify the same single-layer scope and all three Escape/focus/outside
cancellation paths. Main-menu text scope/counts are fully painted, with all
non-whitespace glyphs present and positive bitmap UVs. The tests also cover
locked primary/secondary layers, read-only mode, missing/malformed targets,
empty selection/effects, live eligibility refresh, primary-only selection,
empty selected layers, and duplicate invalid selections.

Product `cargo check` passed with 19 existing warnings. New-file rustfmt,
`git diff --check`, clean-room provenance, and panel style-consumer checks passed.
No colors, dimensions, token values, new styles, control types, or hit tolerances
were introduced. Exact-head CI and the merge receipt remain pending.

| Audit ID | Classification and evidence |
|---|---|
| ER-SCOPE | New functional entry and explicit scopes/counts; real actions and actual glyph paint verified. |
| ER-ELIGIBILITY | New shared whole-selection eligibility; normal unavailable branches dispatch no partial batch. |
| ER-CANCEL | Existing owner cancellation retained; real main/floating route probes pass. |
| ER-ATOMIC | Existing engine rollback/history boundary; actual mid-batch Err preserves state and prior undo. |
| ER-STORAGE | Existing persistence; actual save/reopen preserves selected removals and unselected effects. |
| ER-OBSERVE | Existing shell/engine observations retained; no new logging site or failure-suppression mechanism. |
| ER-PENDING | CI, physical-window/DPI visuals, and remaining #265 recent/favorite/preset work are not completed by this document. |

The fixtures reuse the repository's independently authored `soft_sphere_motion`
graph and existing core/package APIs. No third-party font, icon, image, shader,
asset, documentation text, or compatibility name is copied or bundled.
PROVENANCE.md, ATTRIBUTION.md, NOTICE, and DESIGN.md were checked.

## DESIGN §0.7 disposition

These dispositions describe this patch's scope, not a claim that old violations
have been fixed elsewhere. Existing property rows, control types, hit tolerances,
menu geometry, token values, and style names are not changed.

| Baseline row | Disposition |
|---|---|
| Path six literal row steps | retained with reason: path rows are outside this menu action. |
| Path two-column sizing | retained with reason: no column layout edits. |
| Path button/field heights | retained with reason: no control geometry edits. |
| Path numeric background | retained with reason: no numeric editor edits. |
| Path boolean full-width button | retained with reason: no boolean editor edits. |
| Effect row height/steps | retained with reason: only context action data changes, no property rows. |
| Effect boolean buttons | retained with reason: parameter controls are untouched. |
| Effect enum wide button | retained with reason: enum rendering/interaction is untouched. |
| Effect hover alpha | retained with reason: existing context row state/style renderer is reused. |
| Effect indentation/hierarchy | retained with reason: no property-tree layout changes. |
| Timeline label gutter coordinates | retained with reason: preserve the compatibility baseline. |
| Timeline snap guide color | retained with reason: preserve existing drag feedback. |

DESIGN §12 acceptance is limited to the menu scope, enabled state, cancellation,
actual text paint commands, model changes, storage round trip, and undo verified
below. Physical native windows, every DPI scale, all panels, preview/GPU timing,
and global visual remediation remain pending rather than implied by compilation.
