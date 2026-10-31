# Actual bounded full-volume prerender control

Issue: [#82](https://github.com/MAD-Producer/Kilonova/issues/82)

Status: portable control applied and compiled on Windows; actual Linux ENOSPC
execution is pending CI. Normal and optimized Python both reject Windows before
creating artifacts or mounting anything. This does not complete #82 or qualify
Windows/macOS full-volume behavior.

The Linux desktop CI launcher mounts its own 8 MiB tmpfs with a bounded inode
count and nodev/nosuid/noexec. It verifies the actual capacity and independent
device before starting one exact ignored native test. Ordinary unit tests do
not run the exhaustion control. Evidence stays outside the bounded volume.

The test obtains a real OS ENOSPC by writing at most 16 MiB inside that verified
mount. It preserves an existing native cache frame, asks the actual background
worker for an uncached frame, and checks native fallback pixels, the original
OS error in disk statistics, no published partial entry, unchanged saved
project bytes and a successful project reload. It checks that an existing
cache hit still works while full. Removing only its own filler then verifies
that native rendering, cache storage and a subsequent hit recover.

The launcher records raw stdout/stderr, exact test execution/counts, mount proof,
the native result and cleanup status. It uses ordinary unmount and nonrecursive
empty mountpoint removal. Cleanup failures retain the primary failure, and a
still-mounted directory is never recursively removed. No host drive is filled,
no existing cache/project is deleted, and no Android validation is introduced.

The existing partial-write injection remains a separate control. This new test
does not prove physical storage hardware durability, strict input P95, native
crash causes, or final integrated-source regression and cross-platform CI.

The Linux native test is explicitly ignored by ordinary unit runs and must run
through the bounded-volume launcher. Its expected error, fallback pixels,
existing-hit and recovery assertions have compiled, not yet executed. The
additional source preserves the existing project-directory fixture contract.
CI writes its evidence locally on all events and uploads it only outside PRs,
preserving the repository's established PR artifact policy. The two affected
workflow-policy modules passed (15 and 14 tests) after the one-line condition
repair; no existing test assertion changed. Partial evidence and remaining
native-fault-sink limits are recorded in
[the disk-cache report](native-prerender-disk-cache-current.md).

## Failure evidence in PR job logs

The launcher now checkpoints its actual child PID and exit code before parsing
or mount cleanup. On failure it emits at most the first and last 4 KiB of each
original stdout/stderr file to stderr, while retaining the complete files. This
keeps the failure cause visible when PR artifact upload is intentionally disabled.
Only anchored Rust final test summaries count as executed tests. Neither mount
behavior, the native test assertions nor the PR upload policy changed.

Three actual Python-child controls first failed, then passed unchanged after
the launcher repair: original causes from both streams and persisted exit code,
rejection of summary words without a real test summary, and bounded excerpts
with complete original logs. The test file was renamed with the established
`test_ci_` prefix and registered in the explicit tool inventory; its test bytes
remained identical. These are launcher IO controls, not Linux ENOSPC acceptance.

Two further controls exercise actual children writing non-UTF8 bytes: a failed
child must preserve its exit code and both original causes, and a successful
child must still supply an anchored Rust final summary. Both controls initially
failed because strict stdout decoding raised before the failure excerpts. The
launcher now decodes only its summary view with replacement; complete raw files
remain unchanged. The identical five-test module then passed, with its SHA-256
checked against the failing run. No native assertions or acceptance thresholds
changed. This repairs failure evidence, not any retained native crash cause.
