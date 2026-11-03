# Expression frame admission: cold source preparation

Status: scoped fix verified; final branch integration, full regression and
cross-platform CI remain pending. This does not close issues 85, 86 or 88.

PR 599 head `162edfed` failed in the Linux Rust/GPU job while the unchanged
`merge_dimensions` expression test first evaluated frame zero. The retained
error is `expression cancelled or execution budget exceeded`. The original log
does not distinguish its expression deadline from the outer frame deadline.

Direct `Project::evaluated_at_cancellable` started its 100 ms frame clock before
the runtime compiled cold source. Native Sessions already prepared source
before evaluating frames. A controlled one-shot barrier on actual cold
compilation reproduces the same error class: hold compilation for 120 ms, then
release it without running any expression while blocked. The original code
fails; the new code evaluates the real expression and samples its result.

The direct project API now shares source preparation before starting the frame
clock. A pre-cancelled request rejects before compilation; preparation errors
retain the requested frame, target and original cause. The public preparation
API retains its existing frame-zero diagnostic context. Expression execution
still has 20 ms, and evaluated frames still have 100 ms. Cache capacity, memory
limits, JS isolation and dependency-cycle checks remain intact.

The compile barrier exists only under cfg(test), on the owning thread, and is
removed from production builds. Three new tests cover cold admission,
cancellation before compilation and native syntax causes at a fractional frame.
The full core unit target and seven existing expression/merge integration
targets pass: 81 tests passed, zero failed, one existing unit test ignored.
Existing assertions, error expectations, budgets and ignore markers are
unchanged. Public preparation tests prove compilation does not execute user
code; sandbox and existing timeout/cancellation tests still reject interrupted
execution and permit later recovery.

The evidence archive and adjacent receipt retain the actual original CI log,
controlled pre-fix failure, post-fix runs, source snapshots and environment
wrappers. Every archive member was read back and SHA-256 checked. Initial and
final source identities remain separate; failed records were not rewritten.

The fix was developed in an independent worktree based on `162edfed` while the
original complete GPU/desktop gate remained source-frozen on that old head.
That running gate does not validate this change. No panel, UI layout, asset,
dependency, Android code, worker timeout or restart changed. Final integration
and CI are required before claiming the original PR check resolved.
