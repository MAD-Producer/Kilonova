# Exact-head native prerender PR CI

Issue: [#82](https://github.com/MAD-Producer/Kilonova/issues/82).
PR: [#575](https://github.com/MAD-Producer/Kilonova/pull/575), still a draft.
Tested source: `8bb44d4a81f67f693a20e7bc3c067a987d37cb31`.
[CI run 41169667594](https://github.com/MAD-Producer/Kilonova/actions/runs/41169667594)
completed successfully. All five required jobs passed: dependency audit, Rust/GPU,
Linux desktop, Windows desktop and macOS Intel desktop. Android is retired and
excluded as instructed.

The actual final summary lines in each original job log report:

| Job | Summary lines | Passed executions | Failed | Ignored |
| --- | ---: | ---: | ---: | ---: |
| [windows](https://github.com/MAD-Producer/Kilonova/actions/runs/41169667594/job/124444681695) | 56 | 2010 | 0 | 53 |
| [macos-intel](https://github.com/MAD-Producer/Kilonova/actions/runs/41169667594/job/124444681609) | 55 | 1957 | 0 | 6 |
| [linux](https://github.com/MAD-Producer/Kilonova/actions/runs/41169667594/job/124444681546) | 55 | 1957 | 0 | 6 |
| [rust-gpu](https://github.com/MAD-Producer/Kilonova/actions/runs/41169667594/job/124444681439) | 296 | 1970 | 0 | 7 |

These are observed execution totals, not unique test counts. Jobs overlap;
never add their totals to claim coverage. Rust/GPU includes separate commands.
Explicitly ignored native performance studies remain outside ordinary CI.

The Linux job separately passed [actual bounded-full-volume recovery](native-prerender-full-volume-current.md).
Desktop PR jobs also passed `tools/build_desktop.py --task check`. By policy,
this PR run does not execute arm64, FFmpeg release packaging, packaged video
verification or the main/tag publication chain. It is not four-platform release
qualification; #87 has its independent [release evidence](scene3d-main-release-current.md).

## Acceptance boundary

The exact-head CI requirement is satisfied for this source only. Local source
commits `71a2a696`, `b1daab53` and `3aaab948` have not been integrated into the
PR and are not covered by this run. Their local scoped evidence remains separate.

Current-source local desktop regression has completed 1,463 passed, zero failed
and 51 ignored, including the two tests that failed on the older source. The
rest of that local FFmpeg/AI-depth workspace gate was still running when this
record was written; it is not reported as accepted. The strict 480-sample input
assessment still fails the second property pair (ratio `1.1349866349866349`).
Old illegal-instruction, stack-overflow, Naga precondition and child-exit evidence
remains unresolved. Current CI and changed-source passes do not explain it.
The draft must not be merged or #82 closed on these CI results alone.

## Evidence

Archive: [exact-head job records](data/native-prerender-ci-head-v1.zip).
SHA-256: `971f2a3554e7aa37fed6fc6c601bd50a1f72054cfb7314705b81251962d1de50`. All 12 members were read back byte for byte.
The manifest binds final run/job identities, original command/summary excerpts
and tested workflow source. Full original logs are retained separately with
SHA-256 values in the receipt. No private dumps, executables, PDBs or machine
hardware records are included.
