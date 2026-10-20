# Media probe completion cleanup

## Scope and coordination

This batch addresses the successful audio/video probe completion sites in issue #131, section VI, against main `d9cc2309`. The ten open PR file lists, eighteen recent local branches and uncommitted changes to the intended media paths were checked before creating the separate `fix/media-probe-cleanup` branch. No current branch or PR owns these paths. Draft #344 retains its rendering/sampling work; preferences, custom shortcuts and the other open PRs are left untouched.

The baseline already releases the task guard on the explicit worker-cancellation return and emits ordinary result/recovery observations after releasing that guard. Those are existing protections. The remaining successful probe branch drops its `Prepared` value at the end of the locked match arm; its `Stage::drop` calls `remove_dir_all` and can also call the host logger on failure while the task mutex is still held. Large staging trees can block status queries, and an embedding logger that queries task state can encounter the same lock.

## Regression method

The existing worker-completion blocks are first extracted into private functions without changing their statements or outcome handling. The real workers call these functions. The regressions call that same completion path with owned staging resources and independently authored metadata, not a substitute successful completion implementation.

Two isolated subprocess cases supply a real filesystem cleanup failure (a regular file where a staging directory is expected). The existing Warn record invokes a capture logger at the default Info level. The logger uses `try_lock` to verify that the task is accessible and still nonterminal during cleanup. No blocking lock or panic occurs inside the logger. Assertions after completion also verify that metadata, successful status and the cleanup warning retain their previous meanings; a cleanup warning alone must not turn a successful probe into a failed import.

Two further cases cover actual successful directory deletion, ready imports retaining their staging resources for commit, already-cancelled results remaining cancelled, and original decode errors remaining visible. This is completion-state and filesystem coverage, not a new codec or GPU acceptance claim. Existing media transaction tests remain required.

## Acceptance

The successful probe result is now converted into metadata and its `Prepared` resource is dropped before acquiring the task mutex. Only after cleanup returns does completion publish `succeeded` / `complete`. An import retains its complete prepared resource and continues to report `ready` / `awaiting_commit`. Original errors, caught panic detail, cancellation flags, metadata fields and existing cleanup-warning severity are preserved. The video match is exhaustive without its previous impossible catch-all panic.

This is a cleanup-order fix, not a new observation site: the existing Stage cleanup warning is retained and now runs outside the task lock on these two paths. Cleanup failure remains best effort and does not turn an otherwise successful probe into a failed import. No logger or protocol reply implementation changes.

- Baseline after extraction only: **2 passed, 2 failed**, exit 101. Both failures observed the task mutex held during the actual Stage cleanup warning. Evidence: `artifacts/issue-131-probe-cleanup-before.log`.
- After moving probe cleanup: **4 passed, 0 failed**, exit 0. The same failure callbacks can acquire the task mutex and see nonterminal status before completion publishes success. Evidence: `artifacts/issue-131-probe-cleanup-after.log`.
- Complete `cargo test -p motion-media --locked -j 1 -- --test-threads=1`: **85 passed, 0 failed, 2 ignored**, twelve result summaries including doc tests, exit 0. Existing audio backend, video transaction, cancellation, publication rollback, panic-boundary and default-level logging tests passed. The existing ignored cases require external platform/large-video fixtures and were not claimed as executed. Evidence: `artifacts/issue-131-probe-cleanup-complete.log`.
- New test files pass `rustfmt --check`; whitespace, the desktop style-consumer check and the provenance guard (1,468 tracked paths) pass. No existing assertions were changed. No files under `desktop/app/src/panels` changed.

Runs use locked dependencies, portable MSVC, one build job and one test thread, with the dedicated target `E:/Dev/aem/target/media-probe-cleanup-validation`. Local debug symbols and incremental compilation are disabled through environment variables; debug assertions remain enabled and repository profiles are unchanged. This is local Windows acceptance; remote CI must still pass before merge.

## Remaining scope

Issue #131 remains open. Cleanup from cancel, commit, task/registry destruction and package publication, decoder lifetime coordination, platform-specific decode boundaries and the other issue sections require separate completion evidence. This batch must not be reported as removing every filesystem operation from every media lock. No UI, layout, style, dependency, asset, worker timeout or restart is in scope.
