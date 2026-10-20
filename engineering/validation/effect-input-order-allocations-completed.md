# Allocation-free image ordering for plans without snapshot dependencies

## Scope and coordination

This batch addresses the no-dependency traversal allocation in issue #126 against main `8dbd58c0`. Before implementation, the complete file lists of 14 open PRs, the scoped changes of 18 recent branches, and every dirty worktree were checked. None owns these three files or this traversal change. PR #370 owns separate image-cache eviction scratch; the native CI fixture correction belongs to #368. Existing effect-pipeline work in another worktree is retained separately.

Previously, `image_input_order_for` allocated a zeroed mark array for every nonempty draw list, even when no effected image input, track matte or materialized receiver needed traversal. The returned order was empty in that case.

The older issue also describes computing the order twice. Current main already keeps one successful order across snapshot preparation and encoding; that is existing coverage, not introduced here. A pipeline-error rebuild can still require recomputing a changed plan.

## Implementation and boundaries

The mark array is now initialized on the first valid visit. A plan with no traversal roots allocates neither marks nor output storage. Dependency index checks use the draw count before indexing marks. Effected inputs, mattes and explicit materialization targets still use the same visitor, postorder, cycle checks and error text.

This is a CPU allocation change only. It does not add logs, caches, retained observation keys, GPU resources, dependencies, user-facing strings or UI behavior. Plans that require snapshots still allocate traversal storage. This is not a claim of allocation-free rendering or a measured FPS improvement, and it does not close aggregate #126. Group opacity buffers, dependency-bearing traversal reuse, presenter resources and other frame allocations remain separate work.

## Regression evidence

The new standalone integration-test binary observes `alloc`, `alloc_zeroed` and `realloc` on its own test thread. Other test threads are excluded. The observer delegates to `System`, resets after its scope, and has a positive control that allocates a real vector. Fixtures are constructed before observation; there is no timing threshold or GPU requirement.

- With the original production implementation, the four new tests report **3 passed, 1 failed, exit 101**. A 128-layer plan without image dependencies allocates 120 times over 120 queries, contradicting the zero-allocation assertion
- After lazy initialization, the initial targeted run reports **4 passed**. The allocation check covers ordinary inputs, resource inputs and source-stage layer inputs, plus an empty plan
- Order and failure checks cover reversed dependency chains, shared dependencies, repeated mattes, self/multi-node cycles, invalid image and matte indices, and a subsequent healthy traversal
- A compiling negative control that omits recursive dependency traversal is rejected by the order regression with exit 101. Production source is restored byte-for-byte in `finally`

Production SHA-256 after restoration: `7f948169ef4ebaacb86e0f2b4c8f9c33008c967caa52213631c3f80a8137f99f`.

The allocation observer does not instrument GPU or native-driver allocations. The new standalone tests do not directly call the crate-private materialized-receiver entry point; the full existing renderer suite exercises materialization and pixel behavior. No existing assertions or ignore annotations are changed.

## Acceptance

Windows, portable MSVC, `--locked -j 1`, with local test/dev debug information and incremental compilation disabled. Debug assertions remain enabled.

- `cargo test -p motion-render --locked -j 1 -- --test-threads=1`: **293 passed, zero failed, one existing optional local-image probe ignored**, across 64 result summaries including the doctest target
- The restored full suite includes the final four new tests, all 12 existing layer-materialization tests, real effected-image inputs and mattes, exposure, nested composition and GPU pixel regressions
- The full run reports 420.16 seconds of test execution, after 3 minutes 2 seconds of compilation/linking. The four new tests report 0.00 seconds; these are local observations, not a performance guarantee
- `cargo check -p motion-desktop --locked -j 1`: passed, with 21 existing warnings
- Panel style-consumer guard, provenance guard (1,530 tracked paths), new-test formatting and staged whitespace checks: passed

Cross-platform acceptance remains subject to this head's actual CI results.

Publication coordination was repeated against the same main revision with all 14 open PR file lists, 18 recent branches and dirty worktrees. Another worktree started camera lens/settings changes during validation; its changed files were reviewed and do not overlap this scope. It remains untouched. No panels are modified by this branch.

Local evidence: `E:/Dev/aem/artifacts/issue-126-input-order-{before,after,mutant-transitive,complete,desktop-check}.log`. The coordination report and mutation driver are retained under `.tools/issue-closure-review/126-input-order-*`, outside the versioned release tree.
