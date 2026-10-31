# Native input draw attribution: diagnostic complete

Issue [#82](https://github.com/MAD-Producer/Kilonova/issues/82).
Base `8bb44d4a81f67f693a20e7bc3c067a987d37cb31` plus the four source files in
the evidence archive. The diagnostic gate completed at
`2026-10-31T06:21:01.665569+00:00`, with status
`diagnostic_complete_not_a_fix`. It does not claim performance acceptance.

Test-only markers split actual `Shell::draw_commands` into exclusive preparation,
chrome, diagnostics, overlays and input-cursor intervals. A preallocated buffer
retains at most 64 markers per sample; serialization and file IO occur outside
the measured foreground interval. Production application builds exclude this
instrumentation. A real test passed the inactive, bound, suppression and reset
controls. Existing study assertion expressions and scenario bytes were checked
unchanged; no panel source or interaction behavior changed.

All 480 actual native input samples were verified. Each contained exactly five
draw stages per actual call and no dropped marker, with stage sums bounded by
the enclosing measured draw interval. The report retains the actual foreground
P95 sample for each episode, rather than summing unrelated per-phase percentiles.

| Scenario | Disabled episode 0 | Enabled episode 1 | Enabled episode 2 | Disabled episode 3 |
| --- | --- | --- | --- | --- |
| Property foreground P95, microseconds | 35,265 | 35,156 | 41,152 | 46,115 |
| Timeline foreground P95, microseconds | 35,456 | 33,921 | 35,628 | 34,452 |

Chrome accounts for most draw time in those foreground samples. This identifies
where foreground time was spent, not why it changed or whether background work
caused the variation. Property pairs passed this diagnostic run, but the second
timeline pair was `1.0341344479275514`, so the original strict assessment still
failed. This changed-instrumentation result does not erase the earlier v7
property regression or establish a production fix. No unchanged rerun to seek
a green result is justified by this record.

[Raw samples, attribution, unchanged assertions, original strict failure and instrumented source](data/native-prerender-draw-phases-v1.zip):
22 members, SHA-256
`03016876a4be175c4656de4990cd7d3b925a12080bc6e499d053dcd92f214f10`.
Every member was read back byte for byte. Private binaries and profiles are
excluded. Final source integration, performance repair and acceptance remain
open; this diagnostic alone cannot close #82.
