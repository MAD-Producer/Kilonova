# Published depth source 687cfda6: cross-platform CI

Exact published source: `687cfda6835d575156b461e8a52b3103d1d2b9ca`.
[Run42619281230](https://github.com/MAD-Producer/Kilonova/actions/runs/42619281230)
is terminal success: eight applicable jobs passed and two policy skips. This is
source-bound evidence for that published commit, including adapter observations
and shader-failure fingerprints. It does not cover the later physical agreement,
direct placement or physical aligned-transition commits.

| Job | Terminal conclusion |
| --- | --- |
| Dependency audit | success |
| Rust and GPU | success |
| Desktop tests (ubuntu-24.04, linux-x86_64, motion-studio, x86_64-unknown-linux-gnu) | success |
| Desktop tests (windows-2022, windows-x86_64, motion-studio.exe, x86_64-pc-windows-msvc) | success |
| Desktop tests (macos-15-intel, macos-x86_64, motion-studio, x86_64-apple-darwin) | success |
| Desktop libav backend | skipped |
| Desktop build (windows-2022, windows-x86_64, motion-studio.exe, x86_64-pc-windows-msvc) | success |
| Desktop build (ubuntu-24.04, linux-x86_64, motion-studio, x86_64-unknown-linux-gnu) | success |
| Desktop build (macos-15-intel, macos-x86_64, motion-studio, x86_64-apple-darwin) | success |
| Publish desktop release | skipped |

The Windows desktop test job completed successfully after roughly65 minutes;
its later desktop build job and both other platform build jobs also completed.
A completed platform test job alone was not treated as a terminal run.
Desktop libav and release publication were skipped under the existing PR policy.
No CI run was canceled or retried to publish the subsequent source changes.

The dependency, Rust/GPU and all three completed desktop test logs were downloaded
and individually SHA-256 recorded. Private inventory, original logs and terminal
GitHub run/job snapshots: `artifacts/pr-queue/depth75-ci687-completed-tests-v1/`.
No later-source test pass is inferred from this run. Original local native,
complete-model memory, quality/cross-resolution and thermal failures remain
recorded at [the acceptance entry](depth75-status-current.md). No learned-model
or production-tier qualification follows from this CI result.
