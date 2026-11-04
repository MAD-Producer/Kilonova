# Issues 85 and 86: source-bound cross-platform CI

The five required checks succeeded for each recorded head below: dependency
audit, Rust/GPU, and desktop Linux, Windows and Intel macOS. The original ten
job logs and step metadata were recovered and verified, rather than inferring
success from an empty failure count. These are historical results for those
heads, not results for subsequent main-alignment or fixture-fix commits.

| PR | Head | Actual CI checkout | Desktop binary results |
| --- | --- | --- | --- |
| #597 | `757f060760301efc9dd0595993ebf78de7072946` | `26843d9`, merging that head into `0790404f` | Linux/Intel macOS: 1653 passed, 4 ignored; Windows: 1700 passed, 69 ignored |
| #599 | `b38e4275828ffc0377ffe305b2cab05a68032fbe` | `b2771f8`, merging that head into `757f0607` | Linux/Intel macOS: 1652 passed, 4 ignored; Windows: 1699 passed, 69 ignored |

Every listed binary result has zero failures and zero filtered tests. These
counts describe individual desktop binary runs; they are not a sum of all
workspace targets or duplicated child-process summaries. Existing ignores were
retained. The full logs also retain the other package and GPU targets.

The Windows logs contain the successful native rational-clock test after
process isolation. This closes that particular `RecreationAttempt` regression
on the recorded heads. It does not identify or repair the earlier precision
menu stack overflow, professional-screen heap failure, or local compiler
allocation failure. Their original records remain unchanged.

PR policy omits the FFmpeg feature. Consequently these green runs do not prove
FFmpeg tracking/replacement integration. After aligning both local branches to
main `0790404f`, the separate feature-enabled check found two `E0277` errors in
native video test fixtures that still converted `project.fps` to `u64`.
Those errors and their subsequent precise-clock corrections are recorded
separately; no old green result is transferred to a newer head.

The [receipt](data/frame-rate85-86-green-ci-v3.json) and
[archive](data/frame-rate85-86-green-ci-v3.zip) retain all ten original logs,
job steps, commands, checksums and a read-back-verified member manifest. Two
earlier log-retrieval attempts failed at transport/cache access; their receipts
are retained. Retrieval then used the job-log API directly, with no CI or test
rerun and no new C-drive cache. Android and macOS arm64 are outside this proof.

Current-head checks and remaining issue-level acceptance are still required.
