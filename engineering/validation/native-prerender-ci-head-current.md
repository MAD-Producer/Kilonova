# Exact-head native prerender PR CI

Issue [#82](https://github.com/MAD-Producer/Kilonova/issues/82), draft
[PR #575](https://github.com/MAD-Producer/Kilonova/pull/575).
Tested source: `652fbe6525f5813a85620fe0d5d2ad4eef5f9bfb`.
[CI run 41177231677](https://github.com/MAD-Producer/Kilonova/actions/runs/41177231677)
completed successfully. All five required jobs passed.

| Job | Actual job ID | Passed / failed / ignored executions |
| --- | ---: | --- |
| Rust and GPU | 124467192970 | 2041 / 0 / 7 |
| Desktop build (ubuntu-24.04, linux-x86_64, motion-studio, x86_64-unknown-linux-gnu) | 124467193099 | 2015 / 0 / 6 |
| Desktop build (windows-2022, windows-x86_64, motion-studio.exe, x86_64-pc-windows-msvc) | 124467193121 | 2068 / 0 / 60 |
| Dependency audit | 124467193175 | 0 / 0 / 0 |
| Desktop build (macos-15-intel, macos-x86_64, motion-studio, x86_64-apple-darwin) | 124467193185 | 2015 / 0 / 6 |

These are observed execution totals from raw log summaries, not unique tests.
Jobs overlap; do not sum them as coverage. Both libav and release publication
were skipped by PR policy. Arm64 and FFmpeg packaging/video publication remain
outside this PR run. Android is retired and excluded.

Raw Linux, macOS Intel and Windows logs contain actual successful native worker
priority controls before source preparation. Windows also reports a pass for the
native effect-search test that failed in the local FFmpeg/AI-depth workspace.
A different-machine/features pass does not diagnose the local access violation.
The [combined local gate](native-prerender-combined-gate-current.md) remains
failed: strict foreground input P95, native UI regression and MCP startup.
Uncommitted upload/input-admission changes and subsequent message-only changes
are not covered by this head. The final changed source requires its own CI and
full acceptance. PR #575 must remain a draft until those failures are resolved.

## Evidence

[Exact head, terminal jobs and complete original logs](data/native-prerender-ci-652-five-checks.zip):
eight members, SHA-256
`261a271caab7828af3a897c06cfb6066d9343d1448a776771284788535e68204`.
Every member was read back byte for byte. The first collector stopped before log
collection because it did not classify the seventh skipped publication job;
its incomplete receipt is retained locally. The corrected collector verified
all five original required jobs and both skipped jobs; no CI test was rerun.

The [older exact-head archive](data/native-prerender-ci-head-v1.zip) remains
historical evidence for `8bb44d4a`, not qualification for newer changes.
