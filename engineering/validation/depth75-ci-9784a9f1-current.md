# Terminal original three-platform CI (#75)

[Run 42734644701](https://github.com/MAD-Producer/Kilonova/actions/runs/42734644701)
completed on source `9784a9f1455207e709b1cfcfd99580399fd6e48e` with an overall
**failure**. Its original Windows execution was allowed to finish; no run was
canceled or replaced before its terminal result was collected.

| Original job | Terminal result |
| --- | --- |
| Desktop tests: Ubuntu, Windows, macOS Intel | All three successful |
| Desktop build/check: Ubuntu, Windows, macOS Intel | All three successful |
| Dependency audit | Successful |
| Rust and GPU | Failed during Python test-inventory admission |
| Desktop libav backend and release publication | Skipped by original policy |

The failing inventory reported `test_ci_metric_depth_export.py` as unclassified.
Commit `6c6d8c7a` registers that test; it does not delete it, alter assertions or
change the CI success predicate. Later complete local discovery on the exporter
closure correction ran 159 tests: 158 passed and one original skip. Its first
phase-capture failure and corrected result remain in the
[separate source-bound record](depth75-metric-log-close-current.md).

The [terminal archive](data/depth75-ci-9784a9f1-terminal-v1.zip) and
[manifest](data/depth75-ci-9784a9f1-terminal-v1.json) preserve the original API
snapshot, failed Rust/GPU log, complete three-platform test logs, extraction
controller and summary. Every archived member passed CRC and byte-for-byte
readback. Raw summary totals can include nested subprocess summaries; they are
explicitly not a count of distinct tests. The archive is 404,880 bytes, SHA-256
`2397fa71ac729e6d7e2360587ce565e0536a56b57ab9061c6e264d8c06f23541`.

These old-source platform passes do not qualify later changes. No arm64 release,
package publication, original native crash root cause, complete measured device
memory, thermal stability or learned-model domain quality is established here.
In particular, the authorized metric candidate still fails both unchanged
physical sensor bounds. PR #651 remains a draft and #75 remains incomplete.
