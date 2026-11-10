# Retained Rust-remap and sustained scheduler: terminal CI

Draft PR #651 source `d197552823bef0fe3bc192f01649753e059dbd8c` completed
[run42595433015](https://github.com/MAD-Producer/Kilonova/actions/runs/42595433015)
successfully. Eight applicable jobs passed: dependency audit, Rust/GPU, and
native desktop tests and builds on Ubuntu, Windows and macOS Intel. The libav
and release-publication jobs were skipped by workflow policy. No CI dispatch,
rerun or cancellation was performed to collect this result.

Unlike the [previous source](depth75-ci-dc17b007-current.md), this source includes
the one-service sustained scheduler and the completed actual-model long-run
record. Eight named controls appear exactly once in each native desktop test
log: six Rust-remap numerical/ownership/identity controls, the host process-memory
accounting control, and
`schedule_requires_complete_cohorts_duration_and_refuses_a_short_capped_run`.
These observations belong to the existing test totals; they are not 24 new
tests or repeated executions. CI does not install external weights or perform
the actual 800-request model run.

The [archive](data/depth75-ci-d1975528-v1.zip) and
[manifest](data/depth75-ci-d1975528-v1.json) retain terminal run/jobs metadata,
eight complete raw job logs, download stderr, the collector and named-control
verification. Archive size is 596,100 bytes; SHA-256:
`e9f6765276e3687e6e9a4857a484b74dcebaf0da2f093708adcdc552f99987e7`.
All 22 members passed complete CRC and byte-for-byte readback, with individual
lengths and SHA-256 hashes retained in the manifest.

Seven completed-job logs were copied only after matching their original
download receipt, length and SHA-256. The missing Ubuntu build log and terminal
metadata were collected after completion. The earlier partial snapshot remains
partial; it has not been rewritten as terminal evidence. Original partial
collector session83421 exited0 (chunk `a253f1`) and its tool result was consumed.
The terminal collector itself exited0 (chunk `3b9a33`), without a running session.

The [local complete workspace command](depth75-host-remap-full-regression-current.md)
remains failed. The desktop matrix does not run every graph/render integration
target on Windows; the general workspace GPU job runs on Linux. Remote success
does not establish the cause of local native access violations. It also does
not qualify the candidate's domain accuracy, thermal behavior, complete device
allocation or production quality tiers. The original one-service long run's
thermal flags, cross-resolution refusals, assertions and thresholds remain
unchanged. Issue #75 stays incomplete and PR #651 stays draft.
