# Preserve native child exit evidence

Issue: [#82](https://github.com/MAD-Producer/Kilonova/issues/82).
The two child wrappers which failed in the [older combined v6 gate](native-prerender-combined-gate-current.md)
reported captured stdout/stderr without the child's exit status. This prevented
reliably associating an OS exception status with each failure. Windows reports
and the same executable timestamp could not establish that association later.

The timing-admission wrapper and recent-menu stale-target wrapper now include
the test context and `ExitStatus` before separately labeled stdout and stderr.
Both original `output.status.success()` predicates, child selections, environment,
fixtures and all product assertions are unchanged. This is test failure evidence
only: no product behavior, logging sink, panel, dependency or process policy changed.
The old crashes remain unresolved; printing their future exit status is not a fix.

## Executed controls

A standalone standard-library Rust control compiled the exact assertion macros
extracted from both original and modified source files. A source check proved
that only those failure messages changed. Each macro handled three real child
processes: success, explicit exit 101, and explicit Windows status `0xc000001d`.
The last uses `process::exit` to check exit-status plumbing; it deliberately does
not execute an illegal instruction and is not evidence of a native crash.

The original macros passed the two success controls and failed all four
nonzero-status evidence checks. The modified macros passed all six controls:
nonzero children still failed the original success predicate, and the caught
failure retained the actual OS exit status plus both original output causes.
No Cargo target or GPU was used alongside the live workspace validator.

The first preparation attempt could not link because it lacked the existing
portable MSVC environment. It executed no controls. Loading the repository's
established environment corrected that preparation; the failed compiler output
is retained separately. These controls do not compile the complete desktop test
target, replace its regression gate, or qualify any historical abnormal exit.
The final integrated desktop regression remains required.

Archive: [child exit-message controls](data/native-prerender-child-exit-evidence-v1.zip).
SHA-256: `0065e85fd5580bb70261f81ed2fd55c344247a057d691e86ae618bf7d7921a1d`.
All 11 members were read back byte for byte. The archive contains the exact
generated assertion sources, controller, original/fixed raw outputs, source
hashes, receipt and preparation-failure disposition. No executable, PDB or dump
is included.
