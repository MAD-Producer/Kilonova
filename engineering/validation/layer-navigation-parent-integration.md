# Layer navigation and configurable property-reveal integration

PR #371 head `f13dd68` is synchronized with #362 head `7d7b2d2`, including the
reviewed #349 property filter. The original navigation implementation, its
ordering semantics and regressions are retained. The only textual conflict is
the keyboard router: both the effective-binding UU sequence and the layer
navigation/order dispatch must survive. The resolved router is byte-identical
to the all-PR tree that passed 1,196 tests with zero failures and two existing
ignores. This is combined evidence, not a new full run of this smaller branch.

The effective animation binding still determines sequence ownership. Single
press behavior, repeated-key interruption, text/IME ownership and panel-owner
context hashing are preserved. Navigation remains limited to its existing
focus/modal guards. No layout, widget, style, return value, GPU resource or
protocol change is introduced by this merge.

Exact-branch timeline and navigation tests are pending. The combined Debug GUI
smoke timed out at its existing 40-second deadline after 50 PNG captures; that
failure is retained. A CI-style Release build is in progress with unchanged
assertions and timeouts. Native screenshots, packaged FFmpeg verification and
final cross-platform CI must retain their own validation scopes.

DESIGN section 0.7 rows are retained with reason: only input dispatch is
reconciled; existing property/effect/timeline geometry is untouched. Section 12
GUI acceptance remains pending. No dependency, asset or copied product text is
added. #362 and its #349/#357 dependencies must precede #371 on main. The camera
child #373 must synchronize this updated head before its final acceptance.

Work takes place in an isolated review worktree. The active navigation and
camera worktrees are preserved. This is an update of the existing PR chain,
not a combined replacement PR.
