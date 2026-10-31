# Combined native prerender gate: v7 failed input acceptance

Issue [#82](https://github.com/MAD-Producer/Kilonova/issues/82), draft
[PR #575](https://github.com/MAD-Producer/Kilonova/pull/575).
Tested source `8bb44d4a81f67f693a20e7bc3c067a987d37cb31`, main baseline
`49890ce0da7d7701eaec5e521d749b562d3ffd47`.
Terminal receipt: `2026-10-31T06:14:22.798456+00:00`.

The source-frozen 33-step gate completed. **32 steps passed; strict native input
assessment failed.** This is not combined acceptance. The
[issue acceptance map](native-prerender-issue82-acceptance-current.md) separates
implemented features, accepted components and open requirements. Android is
retired and excluded by user direction.

## Actual results

| Gate | Actual result | Scope |
| --- | --- | --- |
| Maintained full workspace, FFmpeg and AI-depth enabled | Exit 0; 4,009 passed, zero failed, 63 ignored across 348 final summary lines | Full command on this source; ignored tests remain excluded |
| Desktop target within that workspace | 1,463 passed, zero failed, 51 ignored | Subset, not an additional total |
| Native cache, producer, disk and all four precision layouts | Passed | Exact commands and counts in receipt |
| Priority, source freshness, refresh, static, gaps, pressure, blocked ownership and idle controls | Passed | Cancellation and admission correctness, not foreground latency |
| Native input study | 480 verified samples; no drops | Original ABBA order, assertions and strict criteria preserved |
| Strict input assessment | Exit 2, failed | Second property pair 1.1349866349866349; pooled property 1.0183869406881096 |
| Paced playback | 240 frames, strict assessment passed | Both enabled episodes: 60 actual hits, zero missed-next-deadline samples |
| Desktop check/build and actual shared-window MCP | Passed; nested MCP report `ok: true` | Actual application/tool routing and screenshots, not inferred from empty stdout |
| Provenance and tool inventory | Passed | Source/tool checks |

Workspace command:

```text
cargo test --workspace --exclude motion-android --features motion-desktop/ffmpeg,motion-host/ai-depth --locked --offline --no-fail-fast -- --test-threads=1
```

The other property pair was `0.9920199361594892`. Timeline pairs were
`0.6511806341472771` and `0.7933545302298488`, pooled `0.7669862151004129`.
An improving pair does not override the failed property pair. Paced ratios were
`0.9729916897506925` and `0.8336580613735997`, pooled `0.8471691131121253`.
These observations are local evidence, not a universal hardware SLA.

## Source boundaries and retained failures

The [older failed v6 gate](native-prerender-combined-v6-failed.md) remains
preserved with original illegal-instruction, stack-overflow and nonzero-child
evidence. Current changed-source passes do not establish their causes or fixes.
No original assertion, sample order or threshold was weakened.

After v7, all 73 owned commits were rebased onto main
`f2e9f0e82f625eb0e1095857bf3e98d917bcfbcf`; actual range-diff found 73 equivalent
patches and no changed/dropped/added patches. Commit
`02a19dadbf26b06ebea365775c965e0daa1f0e54` adds three regression tests for
stable-identity roving and frozen native temporal/spatial cache boundaries.
Its component gate passed seven test executions (three host static-interval,
two native spatial-timing and two child wrappers), desktop check, provenance
and tool inventory, completing `2026-10-31T06:24:30.021404+00:00`.
This is not a full regression or CI acceptance for the new main baseline.

Remaining: repair and qualify foreground latency, resolve the retained abnormal
exit dispositions, integrate subsequent changes and run complete final-source
regression, actual shared-window MCP and exact-head cross-platform CI. PR #575
remains a draft and #82 remains open.

## Evidence

[Original failed v7 receipt, complete raw logs, samples, assessments and MCP artifacts](data/native-prerender-combined-v7-failed.zip):
183 members, SHA-256
`995da4e61060c79d9c668e99958cc56c19d58b8d69e02e41a762e8fffe0283ae`.

[Rebased-main boundary component evidence](data/native-prerender-main-boundaries-v1.zip):
19 members, SHA-256
`203d232b21700c6fd9db0c8b43b406a02db2743092e5a8075593e9b6c4ae923d`.

Every archived member was read back byte for byte. Manifests retain original
receipt status, exact source hashes, raw command results and scope limitations.
