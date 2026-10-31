# Combined native prerender gate: v8 remains failed

Current integration: see [main-8573 integration](native-prerender-integration-main-8573-current.md). The latest completed combined result below remains failed; later component passes and rebasing do not qualify the final source.

Issue [#82](https://github.com/MAD-Producer/Kilonova/issues/82), draft
[PR #575](https://github.com/MAD-Producer/Kilonova/pull/575).
Tested source `652fbe6525f5813a85620fe0d5d2ad4eef5f9bfb`, main baseline
`f2e9f0e82f625eb0e1095857bf3e98d917bcfbcf`.
Terminal receipt: `2026-10-31T08:01:59.491136+00:00`.

The source-frozen gate completed 34 steps: **31 passed and three failed**.
Strict input P95, full workspace regression and actual shared-window MCP failed.
The [issue acceptance map](native-prerender-issue82-acceptance-current.md)
retains the complete original scope. Android is retired and excluded.

| Gate | Actual result | Boundary |
| --- | --- | --- |
| Full maintained FFmpeg/AI-depth workspace | Exit 101; 4,137 passed, one failed, 72 ignored across 355 summaries | Original full command, not a component pass |
| Desktop subset | 1,515 passed, one failed, 59 ignored | Included in workspace totals |
| Cache, disk, four precisions and native scheduling controls | Passed | Exact command/count evidence in original receipt |
| Input study | 480 verified samples; no trace drops | Original ABBA order, samples and criteria preserved |
| Strict input assessment | Exit 2, failed | Property pair one 1.0251986313286552; pair two 0.9430862064081136; pooled 0.9954186963581536 |
| Timeline input | Both pairs passed | 0.9662935057804349 and 0.8419275929549902; pooled 0.9756957489235037 |
| Paced playback | 240 frames, strict assessment passed | Pair ratios 0.8562281019693125 and 0.7650231124807396; pooled 0.8273406490777493 |
| Real enabled playback hits | 60 per enabled episode; zero missed-next-deadline samples | Actual native Engine playback, not whole-app input acceptance |
| Desktop check/build | Passed | Does not qualify application startup |
| Actual shared-window MCP | Failed before initialize reply | Child stderr reports main-thread stack overflow |
| Provenance and tool inventory | Passed | Source/tool checks |

Full workspace command:

```text
cargo test --workspace --exclude motion-android --features motion-desktop/ffmpeg,motion-host/ai-depth --locked --offline --no-fail-fast -- --test-threads=1
```

## Retained native failures

The desktop failure is
`shell::effect_application::tests::effect_search_native_pixels_and_default_failure_sinks`.
Its original wrapper omitted ExitStatus. The original executable and PDB were
copied and independently hashed before subsequent builds. Windows Application
records match the executable path, PE timestamp and test interval and report
`c0000005`, unknown faulting module. This is not an exception callstack or a
root-cause attribution. The original dump was unavailable. A single serialized
same-binary diagnostic passed without a second-chance exception; that diagnostic
neither erases the original failure nor establishes a fix. Loaded overlay names
are not evidence of responsibility; no layer, heap or assertion policy changed.

The actual MCP child separately printed `thread 'main' ... has overflowed its
stack`. Its original executable/PDB were preserved and hashed before later
builds. The PE reserves 1 MiB for the initial thread. Static unwind metadata
shows large frames but does not prove which runtime call chain overflowed.
The application-specific diagnostic and a source-backed correction remain open.
Private binaries, symbols, WER and dumps remain outside public source archives.

The [v7 failure](native-prerender-combined-v7-failed.md) and
[older v6 failures](native-prerender-combined-v6-failed.md) remain retained.
Changed-source or other-machine passes do not establish their causes or fixes.
No assertion, sample order or threshold was weakened.

## Remaining qualification

The published head's [five required CI checks](native-prerender-ci-head-current.md)
passed. Those checks do not accept local input P95, the original native failures,
or later uncommitted idle-upload/input-admission changes. The final integrated
source still requires original strict input criteria, full regression, genuine
shared-window MCP startup and exact-head CI. PR #575 remains draft and #82 open.

## Evidence

[Original failed v8 receipt, complete public logs, input/playback samples and assessments](data/native-prerender-combined-v8-failed.zip):
134 members, SHA-256
`db6af6469ee0371eae385db4c0f7e933e015ffb9e1777930c89a551a874c73e8`.
Every member was read back byte for byte. The archive explicitly excludes
preserved executables, PDBs, private WER, minidumps and raw machine hardware data.
