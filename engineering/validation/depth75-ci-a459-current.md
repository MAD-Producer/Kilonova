# Terminal remote validation of depth source a459112a (#75)

[CI run 42565328876](https://github.com/MAD-Producer/Kilonova/actions/runs/42565328876)
completed successfully at exact head
`a459112a1871e81d027ec86037a0c1e7e2832e83` before publishing subsequent local work.
All eight applicable jobs passed: Rust/GPU, dependency audit, and desktop tests
plus builds on Ubuntu, Windows and macOS Intel. The libav and release-publishing
jobs were explicitly skipped by the workflow. No run was canceled to publish
the next source. This PR workflow did not include macOS arm64 or Android.

The terminal API run/job metadata and all eight successful raw job logs are
retained in the [manifest](data/depth75-ci-a459-v1.json) and
[archive](data/depth75-ci-a459-v1.zip): 17 members, 580,550 bytes, SHA-256
`13d503b94a4c162fceec1838023244e81f5d7f883f7eeb2da99d92fdcff2fb86`.
Three logs reuse the previously collected exact job IDs and bytes; the other
five were collected after completion. All members passed CRC and byte readback;
the manifest records each member's length and SHA-256. Collector session 6609
was consumed with exit 0.

This remote success covers the finite retained-context source at a459112a. It
does not cover the later live-service, cache, child-diagnostic or Session commits;
they require their own exact-head CI. It does not erase the original failed
local whole-workspace/host runs or the Python/native access violations. Draft
PR #651 and issue #75 remain incomplete.
