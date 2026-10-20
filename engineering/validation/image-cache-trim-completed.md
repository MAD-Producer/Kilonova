# Bounded image eviction scratch reuse

## Scope and coordination

This batch addresses the remaining eviction-list allocation in issue #126, section IV, against main `8dbd58c0`. Before implementation, the complete file lists of 12 open PRs, 18 recent branches, and every dirty worktree were reviewed. No open PR owns this change. Existing dirty renderer changes concern an effect-pipeline observation API, GPU API migration, or viewport calculation; they do not change image eviction. They remain untouched in their own worktrees.

The healthy-path early return already exists in main and is retained as existing coverage. This batch does not claim to introduce it. Under cache pressure, the previous implementation still collected and discarded a new `(last_access, asset_id)` vector on each trim.

## Implementation and limits

- Each renderer reuses an empty candidate vector between trims. Sorting still uses ascending `(last_access, asset_id)` order, and active images and the current replacement target remain excluded
- Candidate IDs are cleared after use. The retained allocation contains no texture handles and is capped at 1,024 pairs, or 16 KiB of CPU storage; larger candidate lists can still be processed completely, but their storage is discarded afterward
- `clear_assets` and successful preview working-set reclamation release the retained CPU allocation
- Texture budgeting, residency accounting, preview/full-output policy, and the existing accounting-mismatch observation are unchanged

This removes repeated allocation of the eviction vector for a warmed workload within the retention cap. It is not a claim of zero allocations across rendering, a measured FPS improvement, or a change to GPU memory budgets. The CPU scratch is separate from the texture ledger. Sorting, candidate scans, texture destruction, and other cache work still occur when required.

## Regression evidence

The three new tests call the actual renderer trim paths after uploading real GPU images. No GPU absence is silently skipped.

1. Force a one-image eviction with tied access timestamps, an older active image, and an older replacement target. Verify the lower eligible asset ID is evicted, both protected images survive, and the replacement becomes eligible on the next call. Check booked bytes against actual image entries
2. Repeat real pressure 64 times and verify eviction results, exact texture accounting, and the same candidate backing allocation and capacity after warm-up. Follow each pressure trim with a healthy trim that must leave both images and storage unchanged. Reclaim the working set and verify the active image survives while CPU scratch is released
3. Upload 1,025 images, force a complete eviction, and verify an oversized candidate buffer is not retained. Warm a small list, clear assets, and verify both texture accounting and CPU release

The original trim algorithm, with only the inert scratch field and test module added for inspection, passes the policy regression and fails the two storage regressions: **1 passed, 2 failed, exit 101**. After implementation, all three pass. The fixture keeps preview caching enabled while populating the small working set, then disables it to trigger eviction; setup itself must not evict earlier uploads.

Five compiling production mutations independently remove storage reuse, active-image protection, replacement protection, the retention cap, and storage release. All five were rejected by the relevant regression (exit 101), with byte-for-byte source restoration in `finally`. Final production SHA-256: `e416be4f64ac5bf69299cb3e0fada7570222699332fda1b44b07fd49f97b52f9`.

## Acceptance

- `cargo check -p motion-desktop --locked -j 1`: passed, with 21 existing warnings
- `cargo test -p motion-render --locked -j 1 -- --test-threads=1`: passed, 292 passed, zero failed, one existing optional local-image probe ignored, across 63 result summaries including the doctest target
- Existing real GPU image-preview-cache and working-set tests passed, including reverse-seek pixel identity, LRU behavior, video-plane budget reclamation, and full-output cache release
- The three new regressions passed before the mutation checks and again in the restored full suite; all existing assertions and ignore annotations are unchanged
- Provenance guard (1,530 tracked paths), panel style-consumer guard, new-test formatting, and staged whitespace checks: passed

The full suite reported 476.68 seconds of test execution, after 3 minutes 12 seconds of compilation/linking. The targeted three-test run took 5.99 seconds. These are local observations, not a performance guarantee.

Publication coordination was repeated against the same main revision. All 12 open PR file lists, 18 recent branches, and dirty worktrees were checked again; the three previously reviewed overlapping dirty renderer diffs were byte-identical and remain outside this branch. PR #368 advanced its failure-evidence patch without touching this scope.

Local raw evidence is under `E:/Dev/aem/artifacts/issue-126-image-trim-*.log`. The coordination report and mutation driver are under `.tools/issue-closure-review/126-trim-*` outside the versioned release tree.

## Remaining work

This does not close aggregate #126. Vector group uniforms, image-input ordering, presenter binding ownership, cold pipeline waits, and other frame allocations require independent verification and changes. No panels, presentation rules, layout, UI interactions, dependencies, third-party assets, or user-facing strings change in this batch.


## Native CI integration refresh — 2026-10-20

To clear the existing PR queue, this branch now includes the reviewed #368 head
`1381cc1`, including the already merged #364 Intel budget. The PR's Rust
implementation and regression files are byte-identical to the previous head;
the incoming CI/test infrastructure changes are tracked in
`native-ci-failure-evidence.md`. Main's zlib source attribution update is retained
where applicable. No existing assertion, timeout or ignore policy is relaxed.

This is a real baseline change, not a rerun of an unchanged failed head. The
sampling fixture avoids crossing its original 100-second loop during a slow
run while retaining strict adjacent-frame progress checks. PNG diagnostics
publish in-flight, queue and GPU context on the original timeout; adding that
evidence is not described as fixing the underlying PNG timeout. Evidence paths
are absolute and fall inside the always-upload directory.

The prior per-PR validation recorded above remains historical evidence. A local
combined-tree acceptance run is in progress in `artifacts/pr-queue-complete-tests.log`;
it is not claimed as a full successful run or a replacement for this head's CI.
The local combined timeline-shortcut suite passed all 39 tests and the tools
suite passed 54 of 55 checks with one existing optional external-loader skip.
Those combined results include other PRs, not just this individual branch.

The branch is prepared for stacking on #368, then returning to main after the
parent merges. Actual publication and final remote checks must be verified
separately. No new optimization PR, dependency, asset or visual change is added.
