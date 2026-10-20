# Session registry lock scope

## Scope and coordination

This batch addresses issue #130, section II, against main `abfb4e4b`. Before implementation, the complete file lists of 11 open PRs, 18 recent local branches, and dirty files in every worktree were inspected. No open PR owns the session registry change. Historical observation and sampling integrations already present in main are retained; unrelated render/surface changes in older worktrees remain untouched. The directory-cache lock work in #363 is a separate part of the same aggregate issue.

This is a functional concurrency fix, not an additional logging-only audit. The global registry previously stayed locked throughout handlers and session teardown. A blocked handler or destructor therefore prevented unrelated sessions from being accessed, opened, or closed. Session construction was already outside the registry lock and remains there.

## Implementation and lifetime boundary

- The registry owns shared entries containing an immutable owner ID and `Mutex<Option<Session>>`. It is locked only to find, insert, remove, or copy entry references.
- `with_session` releases the registry before running a handler, recovering plugin state, or waiting for the session lock. A foreign-thread call returns the existing owner-thread error before waiting behind a blocked handler. Session thread checking remains in place.
- An owner-thread close removes the entry, takes its session, and releases both guards before detach and destruction. A retained report entry becomes empty: it cannot keep GPU/decoder resources alive until some foreign reader drops the last shared reference. Foreign close remains a no-op; repeated close remains harmless.
- Resource reporting copies entry references under the registry lock, then reads each session separately. Filesystem canonicalization and resource observation emission happen without the registry lock. The report is an enumeration with individually sampled values, not an atomic snapshot of every concurrently edited session. A report itself can still wait for a busy session.
- Existing metadata and registry observation scopes survive both return and unwind. The latched-error event is emitted only after the session guard is released. There are no new logs, retries, handler deadlines, thread migrations, or restarts.

## Reproduction and negative controls

`crates/motion-host/tests/session_concurrency.rs` uses real public session operations with empty projects. The platform fixture rejects unexpected decoding and surface work.

1. A first owner enters a controlled blocked handler. A second thread must reject access to that session with the original error, leave it open after foreign close, then open, edit, and close its own session before the first owner is released.
2. The final session-owned platform reference blocks in its real destructor. Another owner must open, access, and close a session during that block. The observed destructor thread must be the closing owner.
3. Handler errors retain their original cause; a caught handler panic still propagates as a panic; subsequent access recovers the session mutex; resource counts preserve directory scope and reflect close.

The original implementation fails both controlled contention tests and passes the error/panic compatibility test: **1 passed, 2 failed, exit 101**. Cold session initialization has a separate 30-second setup allowance; the five-second contention deadline starts after the blocked phase is entered. Release and thread joins precede contention assertions. The controller's bounded waits are test safeguards, not production operation timeouts.

The new isolated unit regression additionally checks that a real resource report waiting behind a held session has released the registry. Its retained entry is the synchronization barrier; the handler is released and the report joined before assertions. It also checks that handler poison stays local, the registry remains unpoisoned, logging observes both locks released, and closing frees session-owned platform references even while a report-style entry is retained and later dropped on another thread.

All four compiling production mutations were rejected by the regressions (exit 101):

- Delay the owner check until after acquiring the session mutex: another session waits behind the blocked handler
- Retain the registry guard through close: real cleanup blocks unrelated sessions
- Emit the latched-error log while retaining the session guard: the actual sink detects the held session lock
- Retain the registry guard while a resource report waits on a held session: the synchronized report regression detects the blocked registry

Each mutation is restored in `finally`, with byte-for-byte SHA-256 verification before final acceptance.

The existing metadata-unwind fixture explicitly acquires the registry before its intentional panic so it still tests deferred emission after registry poison. All existing assertions are retained. The separate new unit test verifies that an ordinary handler panic no longer poisons that registry.

## Acceptance

- `cargo check -p motion-desktop --locked -j 1`: passed, with 21 existing warnings
- `cargo test -p motion-host -p motion-desktop --locked -j 1 -- --test-threads=1`: passed; desktop 612 passed and one existing native-audio test ignored, host 86 passed including integration tests and the doctest
- `cargo test -p motion-host --locked --features diagnostics -j 1 -- --test-threads=1`: passed, 86 passed across 12 result summaries; this run includes the additional blocked-resource-report checkpoint
- Final default-feature reruns of the isolated registry unit test and public session-concurrency integration target: 1 and 3 passed, respectively, after strengthening assertions to check the original panic payloads
- Provenance guard (1,530 tracked paths), panel style-consumer guard, formatting of the two new test files, and staged whitespace checks: passed
- The original implementation failed both contention regressions; all four compiling production mutations failed their intended regressions; restoration verified the production file SHA-256 `1b2fef87afc24c53bd0e882067235ac8b6febb68f70a7f4f0b017933b91f3b33`

The full desktop run took 476 seconds. No assertion was removed or relaxed, and no additional test was ignored. The only changes after the full feature run strengthen test-only panic-payload assertions; the final targeted reruns cover them. No panel, style, or UI behavior changed.

Publication coordination was repeated against main `8dbd58c0`: the complete file lists of 11 open PRs, 18 recent branches, and all dirty worktrees were reviewed. No open PR touches the five files in this batch; previously reviewed unrelated dirty source changes are retained. Main advanced only in the FFmpeg source-host documentation, attribution, and preparation script, with no Rust or Cargo input changes.

Local raw logs are `E:/Dev/aem/artifacts/issue-130-session-registry-*.log`; coordination and mutation scripts are retained under `.tools/issue-closure-review/130-session-*` outside the versioned release tree.

## Remaining limits

The owning worker still executes commands serially. An indefinitely blocked handler still prevents progress within that session; resource queries may wait for it. Same-session reentry remains unsupported. This batch does not establish device-driver cancellation, leak-free shutdown for every detached worker, or UI responsiveness throughout every slow operation. It does not close the aggregate #130. No panel, layout, style, command reply, public session signature, asset, or dependency changes are included.
