# Image proxy cache directory shards

## Scope and coordination

This batch addresses issue #130 section VIII's process-wide proxy cache IO mutex, against main `d0cfbab1`. Twelve open PR file lists, eighteen recent local branches and dirty intended paths across every worktree were checked before creating `perf/image-proxy-cache-shards`; no overlap was found. The draft sampling work, preferences/shortcuts, FFmpeg source-host repair and the separate media cleanup PRs retain their own files and branches.

The baseline serializes read/header/hash validation and write/scan/eviction for every project's disposable proxy directory under one process-level mutex. This is existing mutual exclusion protecting a per-directory cache budget, not an error in the file checksum or publication format. Unrelated projects consequently wait for the longest cache IO in any project.

## Implementation and limits

The implementation uses a fixed array of 64 mutexes, selecting a shard from the canonical cache file's parent directory. Production cache paths already canonicalize and validate that directory under the project root before use. All filenames in the same directory share a shard so read/write/publication/eviction retain their previous exclusion and budget semantics. Hash collisions conservatively serialize some unrelated directories; this is bounded sharding, not a promise that every pair of projects has a distinct mutex.

No path-key registry or per-project mutex allocations are retained. Hashing a path needs no file IO and no new dependency. Existing proxy byte limits, source hashes, file names, checksum validation, unique temporary files, rollback/cleanup, cancellation and failure-streak resets remain unchanged. The poison observation keeps its existing bounded process-lifetime counter across all shards and reports a bounded shard number after guard release.

## Regression method

Two regressions use actual proxy writes and checksum-validated reads. The first holds one directory's actual cache mutex while another thread reads, overwrites and rereads a different directory. It releases the guard and joins the worker before asserting the result, so a failing baseline cannot strand a blocked worker. A two-second completion bound detects the process-wide lock; it is not a throughput benchmark. Candidate directory selection avoids hash collisions so the case tests actual independence rather than claiming collision-free sharding.

The second checks 128 distinct file keys within one directory cannot acquire its lock concurrently, then runs eight actual writers/readers against that directory and checks every returned pixel buffer and final file count. Existing poisoned-lock, default-Info/Debug threshold, unwind, corrupt-file and cleanup-suppression tests still run. Their lock references target the selected directory shard, while logging callbacks check every shard is free. No existing assertion was weakened or removed.

Parallel testing exposed an existing temporary-collision fixture's assumption that its next process-wide serial cannot be consumed by another directory. Its original `AlreadyExists` and content assertions remain unchanged. That test now acquires all shard locks solely while injecting its exact temporary filename; acquisition uses nonblocking attempts and releases partial locks before retrying, with a five-second test failure bound. This isolates the shared serial without adding a global lock to production or changing temporary-file naming. The new held-lock cases use the real poison-recovering cache-lock helper.

## Acceptance

- Baseline with only a test accessor returning the original global mutex: **1 passed, 1 failed**, exit 101. Actual unrelated-directory IO times out while the first directory is held. Evidence: `artifacts/issue-130-proxy-shards-before.log`.
- Initial post-fix cache group: **7 passed**, exit 0, including original failure/observation cases. Evidence: `artifacts/issue-130-proxy-shards-after.log`.
- An initial incorrect per-file-key mutation survived the original two-filename check because those keys collided. The new case was strengthened to check 128 keys; the same mutation then fails with the wrong lock exclusion, exit 101. Initial and final evidence: `artifacts/issue-130-proxy-shards-initial-key-mutation.log` and `artifacts/issue-130-proxy-shards-file-key-mutation.log`.
- Collapsing every directory to shard zero fails the final cross-directory regression with a real IO timeout: **1 passed, 1 failed**, exit 101. Evidence: `artifacts/issue-130-proxy-shards-global-mutation.log`. Both mutation scripts restore the production file byte-for-byte with SHA-256 verification.
- Initial eight-thread cache-group verification exposes the shared-serial fixture race: **6 passed, 1 failed**, exit 101. After fixture isolation: **7 passed**, exit 0, followed by eight fresh-process eight-thread runs of the same group (**56 successful test executions**, not 56 different tests). Evidence: `artifacts/issue-130-proxy-shards-parallel-before.log`, `artifacts/issue-130-proxy-shards-parallel-after.log` and `artifacts/issue-130-proxy-shards-parallel.log`.
- Final complete `cargo test -p motion-render --locked -j 1 -- --test-threads=1`: **215 passed, 0 failed, 1 ignored**, 53 result summaries including doc tests, exit 0. The existing ignored image probe needs an external fixture and was not claimed as executed. Evidence: `artifacts/issue-130-proxy-shards-complete.log`. An earlier complete run before fixture isolation also passed; the final full suite was rerun after that test change.
- Touched child test/observation files pass `rustfmt --check`; whitespace, the desktop style-consumer guard and the provenance guard (1,470 tracked paths) pass. No panel, UI/style, asset or dependency changes.

Runs use locked dependencies, portable MSVC, one build job and one test thread, with `E:/Dev/aem/target/image-proxy-cache-shards-validation`. Debug symbols and incremental compilation are disabled only through local environment variables; debug assertions and repository profiles remain unchanged. These are local Windows results; remote CI remains required before merge.

## Remaining scope

Issue #130 remains open. Global session registry scope around native/GPU work, per-project blocking IO, decoder/worker lifetime boundaries and UI responsiveness during long serial engine operations are not fixed by this cache change. No UI layout, interaction, assets, dependencies, timeouts, retries or restarts are in scope.
