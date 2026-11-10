# Exact published-source CI after retained depth Session integration

CI run [42573859294](https://github.com/MAD-Producer/Kilonova/actions/runs/42573859294)
completed successfully for source
`53c3280dd83f6d373bd00897e651f0d05b7da50c` before publication of the next three
commits. Eight applicable jobs succeeded: dependency audit, Rust/GPU, native
desktop tests on Ubuntu/Windows/macOS Intel, and desktop builds on those same
platforms. The libav backend and release publication were policy skips.

The native test matrix includes `motion-host/ai-depth` and the explicit GPU
features. The original eight WGSL execution-identity controls were independently
located exactly once as successful tests in each of the three collected native
test logs. They cover both combined identity fields, source-bound transform
resolution, actual transform/native receipt validation, partial/stale/wrong
hardware refusal, canceled attestation and changed native worker refusal.
These are eight named controls per platform within the existing matrix, not an
additional test execution or an additive count on top of its summary.

The [archive](data/depth75-ci-53c3280d-v1.zip) and
[manifest](data/depth75-ci-53c3280d-v1.json) retain the exact run/jobs responses,
all eight successful jobs' raw logs and their collector. All20 members passed
CRC, byte-length and SHA-256 readback:582,943 bytes, SHA-256
`8e0f8674ba0ac9ad2d074892e42782d8171a554ecc6b7136a0ee6da35d7be371`.
Original collector session41814 completed with exit0 and was consumed.

This result covers that source only. It does not cover the later retained
cohort, Rust-remap implementation or its cache/Session controls at `dc17b007`.
External models, weights and inference runtimes are not installed by this CI
matrix. Its green status therefore does not qualify learned-model accuracy,
actual sustained inference, thermal behavior, complete memory or a quality
tier. It does not erase any original failed local full-workspace command.

Current-source CI and the original full maintained-workspace regression must be
inspected separately. Issue #75 and draft PR #651 remain incomplete.
