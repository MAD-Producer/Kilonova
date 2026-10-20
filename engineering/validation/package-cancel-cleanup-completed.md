# Package cancellation cleanup

## Scope and coordination

This batch targets issue #131 section VI, specifically the package worker's cancellation-result destructor. The baseline is main `d9cc2309`. Eleven open PRs, eighteen recent local branches and uncommitted changes in all worktrees were checked before creating `fix/package-cancel-cleanup`; no intended path overlaps were found. PR #359 owns the separate audio/video probe completion work and is not included in this branch.

The package API intentionally acknowledges cancellation as `running` / `cancelling` until its prepared archive is closed and removed. Actual publication must remain atomic with cancellation under the task mutex. Those are existing protocol boundaries, not defects. The baseline also destroys the cancelled worker result under that mutex, preventing status queries and repeated cancellation requests during slow cleanup.

## Verification method

Two controlled cases call the real `PackageJobs::start_export` worker with the real `PackageSnapshot` writer and `PreparedPackage` destructor. A wrapper adds channel gates at destruction or publication; it does not replace archive bytes, cleanup or publication with a successful stub. Gates use five-second failure bounds and a nonblocking task-lock observation. The test releases the blocked worker before asserting on the negative baseline.

The cancellation case requires the task to be accessible and still `running` / `cancelling` during destruction. Status reads must remain possible, release and a second export must be refused until cleanup completes, the previous destination must remain byte-identical, and the real staging archive must disappear before terminal cancellation. The publication case requires the cancellation lock to remain held; cancellation cannot undo an already published result, which is imported back and compared with the original project.

## Acceptance

On the cancellation branch, the worker now releases the task guard before inspecting the caught result and destroying its archive, then reacquires the guard to publish terminal cancellation. The task retains its running/cancelling state and cancellation flag during this interval. Actual publication keeps its existing guard. Original error and panic detail still reach the existing completion logger after the guard is released. No new observation counter, severity, protocol response or logger implementation is introduced.

- The initial test run exposed a new fixture's Windows path spelling mismatch as well as the expected lock failure. The fixture now supplies the canonical destination that the real writer returns; assertions and production code were unchanged for this adjustment. The initial evidence is retained in `artifacts/issue-131-package-cleanup-initial-fixture.log`.
- Corrected baseline with unchanged production code: **1 passed, 1 failed**, exit 101. The cancellation destructor's `try_lock` observation is absent because the task mutex is still held. The actual publication/cancellation boundary already passes. Evidence: `artifacts/issue-131-package-cleanup-before.log`.
- After the cleanup-order fix: **2 passed, 0 failed**, exit 0. During controlled cleanup, actual status reads work, release and overlapping export are refused, and cancellation becomes terminal only after the real archive is deleted. Evidence: `artifacts/issue-131-package-cleanup-after.log`.
- Complete `cargo test -p motion-media --locked -j 1 -- --test-threads=1`: **83 passed, 0 failed, 2 ignored**, twelve summaries including doc tests, exit 0. Existing package, cancellation/error logging, panic-boundary, poisoned-lock and media transaction tests passed. The existing ignored tests require external platform/large-video fixtures and were not claimed as executed. Evidence: `artifacts/issue-131-package-cleanup-complete.log`.
- New test formatting, whitespace, desktop style-consumer checks and the provenance guard (1,467 tracked paths) pass. No existing assertions or panel files changed.

Runs use locked dependencies, portable MSVC, one build job and one test thread, with the separate target `E:/Dev/aem/target/package-cancel-cleanup-validation`. Debug symbols and incremental compilation are disabled through local environment variables; repository profiles and debug assertions are unchanged. This is local Windows acceptance; remote CI remains required before merge.

## Remaining limits

This batch does not release the publication mutex, join workers, change cancellation acknowledgements, add retries/timeouts, or convert cleanup warnings into failed exports. It does not address audio/video cancel, commit and registry destruction, decoder lifetimes or other issue #131 sections. No UI, style, asset or dependency changes are in scope. The issue must remain open.
