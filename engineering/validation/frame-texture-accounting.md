# Combined frame texture capacity increment

Related issues: #60 and #61, prerequisite work for #72, #74, #78 and #87.
Baseline: main `ec6763d4352fd059e617c1059704b80652102379`.

## Problem and implementation

The existing ownership ledger excludes allocations removed from application
ownership but still present in an instrumented pending submission. The existing
per-renderer pending totals cannot simply be added: a shared target can appear
in both preview and editor snapshots. Doing so overstates the combined capacity,
while using ownership alone understates that observed union after replacement.

Each renderer now exposes a bounded metadata snapshot with its existing counters
and pending allocation identities collected under the same observation lock.
`FrameTextureCapacity::observe` merges current ownership with those identities,
deduplicating shared current/pending/cross-renderer allocations. It reports
missing tracking and unknown capacity without converting either into evidence
of zero allocation. The host's explicit `resource_ledger` query exposes this
combined observation and retains all previous fields.

No GPU handles are added to snapshots. The existing maximum of eight tracked
submission snapshots per renderer is unchanged. Disabling tracking preserves
previous pending records. Queries neither wait nor submit GPU work, and ordinary
playback does not gain allocation enumeration or file IO. There are no UI,
layout, style, dependency, asset, project-format or packed-plan ABI changes.

## Coordination

Open PRs were inspected before implementation. #383 owns vector upload reuse and
additional timing/counter instrumentation, and #382 owns composition bundle
boundaries. This increment leaves their upload, bundle and frame measurement
implementations alone. Its renderer edit adds only an explicit snapshot accessor.
#386 independently adds preview target admission; this baseline does not include
that PR and this work does not claim its acceptance results.

## Local acceptance

Implementation source: `8c1fc62e`. Strengthened gap/unknown-capacity tests:
`caaa9d04`. Only the new controlled test file changes between those commits;
production code and existing assertions are identical. The final seven-test
ledger selection was rerun after that test change. Other selections below use
the same implementation and their unchanged test sources. Later edits to this
completion record do not change the tested Rust code.

All builds, temporary files and logs use E. Existing assertions, ignores and
timeouts were not weakened. Complete terminal results:

| Command/selection | Result |
| --- | --- |
| `cargo test -p motion-render --lib submission_ledger --locked -- --test-threads=1` | 7 passed, 0 failed, 0 ignored |
| `cargo test -p motion-render --test submission_memory --test texture_memory --locked -- --test-threads=1` | 8 passed, 0 failed, 0 ignored |
| `cargo test -p motion-host --test memory_policy --locked -- --test-threads=1` | 4 passed, 0 failed, 0 ignored |
| `cargo test -p motion-host --lib --test preview_memory --locked -- --test-threads=1` | 70 passed, 0 failed, 0 ignored |
| `cargo check --workspace --exclude motion-android --locked` | Passed with existing desktop warnings |

The total is 89 passing tests, counting the final ledger selection once.
Provenance, panel-style and diff guards pass. Local logs are retained as
`artifacts/pr-queue/frame-texture-accounting-*.log`, with source and SHA-256
evidence in the local acceptance record. These selections are not a claim of a
full workspace test run or completed native cross-platform CI.

New controlled bookkeeping tests allocate actual GPU targets and verify shared
retirement, replacement with identical dimensions, separate renderer snapshots,
ordered completion/recovery, tracking disable, bounded snapshot gaps and unknown
Depth24Plus capacity. Controlled begin/complete calls make lifetime assertions
deterministic; they are not evidence of physical GPU retirement. Existing real
submission tests independently exercise actual queue completion callbacks and
unchanged captured pixels. The host test queries the real session JSON bridge,
including its caller-owned capture target, runtime tracking and read-only state.

## Remaining gates

This is not a whole-frame reservation allocator, a true transient frame peak,
a hardware memory-pressure provider or a physical GPU residency cap. Per-renderer
snapshots are taken sequentially. Raw submissions, detached contexts, buffers,
swapchain and external decoder allocations still require separate coverage.
The additive API exposes those limits explicitly instead of marking the result
complete. Cross-platform native CI and Android hardware pressure acceptance are
still required. #60, #61 and all four 3D prerequisites remain open.
