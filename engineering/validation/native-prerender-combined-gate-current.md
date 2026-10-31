# Combined native prerender gate: v6 failed

For the latest requirement-by-requirement status, see the
[issue 82 acceptance map](native-prerender-issue82-acceptance-current.md).
This report preserves the older v6 failure and its exact source boundary.

Issue: [82](https://github.com/MAD-Producer/Kilonova/issues/82).
Draft: [PR 575](https://github.com/MAD-Producer/Kilonova/pull/575).
Exact tested source: `cd26ba2964569d765cafb34a89be079ac0506861`.
Main baseline: `49890ce0da7d7701eaec5e521d749b562d3ffd47`.
Completed: `2026-10-31T03:57:50.657055+00:00`.

This is a failed combined gate. The three source-watcher/upload components are
integrated in this source; their focused passes do not override these failures.
The optional disk-cache draft and subsequent input-phase instrumentation are
separate, unaccepted sources. Android is retired and excluded by user direction.
Issue 87 is independently complete; see [its release record](scene3d-main-release-current.md).

## Actual results

The 27-step gate completed without source changes. It exercised scheduler,
native cache/producer, cancellation, input priority, source changes, refresh,
static/gap/pressure/blocked/idle paths, all four native precisions, actual native
input and paced playback, full maintained workspace, desktop check/build,
shared-window MCP, provenance and tool inventory. Exact commands remain in the
terminal receipt. Three required steps failed:

| Gate | Actual result | Disposition |
| --- | --- | --- |
| Integrated native cache | Exit `0xc000001d`, no final test summary | Illegal instruction; unresolved |
| Foreground input P95 | Timeline first pair `1.3568106642791415`, pooled `1.0860751295336788` | Strict no-regression failed |
| Maintained workspace | Exit 101; 3,988 passed, 2 failed, 61 ignored | Failed; not acceptance |

Property-input pairs were `0.9680153954836667` and `0.7013104698173707`,
pooled `0.7381782163689057`. Timeline second pair was `0.6718198887465562`.
An improving pair cannot override another failing pair. The unchanged original
input parser, not a replacement threshold, determined the failed disposition.

The separate paced 240-frame study passed its strict assessment: pairs
`0.9938537185003073` and `0.846324771131455`, pooled `0.8398920829844637`.
The enabled episodes recorded 60 and 59 hits with zero missed-next-deadline
samples. This Engine-only result does not prove whole-app input latency or a
universal hardware guarantee and does not repair older failed studies.

The workspace command was:

```text
cargo test --workspace --exclude motion-android --features motion-desktop/ffmpeg,motion-host/ai-depth --locked --offline --no-fail-fast -- --test-threads=1
```

Its 348 actual final summary lines total 3,988 passed, two failed and 61 ignored.
The desktop target's 1,459 passed, two failed and 51 ignored are a subset,
not an additional total. The failed tests are:

- `engine::failure::composition_tests::timing_admission::unsupported_timing_is_quiet_and_preserves_default_output_channels`
- `shell::effect_application::tests::recent_menu::an_actual_worker_rejects_stale_menu_targets_without_partial_additions_or_extra_history`

The first wrapper captured an explicit child stack-overflow report. The second
captured a nonzero child exit. Neither wrapper printed the numeric child exit
code. Two Windows reports identify the exact same desktop PE timestamp but do
not establish which test generated each report. Original dumps are unavailable.
The bounded failure excerpts preserve this distinction; no assertion was changed.

Desktop check, FFmpeg/AI-depth build, real shared-window MCP, provenance and tool
inventory passed after the workspace step. Shared-MCP verification is in its
nested `mcp-ui-smoke.json`, not its empty top-level stdout. Five required PR CI
checks also passed on the exact head, including macOS Intel. They are separate
from the failed local combined gate and do not authorize closing issue 82.

## Native evidence and limits

The integrated render EXE/PDB were copied and SHA-256 verified before later
builds. Windows reports illegal instruction at RVA `0x1f02033`; the original
runtime exception context/dump is absent. Nearest public symbols and static
on-disk bytes are insufficient to identify the faulting source or instruction.
The last printed cache-copy test is not proof of the faulting test or thread.

The preserved desktop EXE SHA-256 is
`73e7ea773102f6f1f0835064c47bff09aadd0e85274286de5e096fdeb5b8821e`;
render EXE SHA-256 is
`304f134b4ad5477d0e12f4a6853fc9bc4da88d31b480f57198f83e26bd101895`.
Private PDB/Windows reports are retained outside this public evidence archive.

Read-only x64 PE/unwind inspection found large fixed stack allocations in the
preserved desktop image. This is diagnostic evidence, not a root cause: public
symbol names may be approximate, and prologue metadata excludes recursion,
dynamic allocations, call chains and the actual test-thread stack limit. The
image's 1 MiB initial stack reserve does not establish a Rust test-thread limit.
No stack-size increase, layer disabling, retry or source change was used to
turn these failures green. A later debugger pass cannot erase an original crash.

## Remaining acceptance

- Identify and fix the foreground timeline-input regression from measured phases
- Resolve/dispose of the retained native crashes and both desktop failures
- Finish the separately drafted optional disk cache, quota/reopen bounds,
  corruption/fallback controls and real bounded-full-volume recovery
- Integrate the corrected sources, align with current main and complete full
  maintained regression plus exact-head cross-platform CI

The earlier v5 failed receipt/archive remains preserved. Its offline metadata
cache repair is not a product fix; the dependency-boundary control passed in
this newer workspace. That pass does not erase v5's original failure or crashes.
PR 575 remains a draft and issue 82 remains open.

## Evidence

Archive: [failed combined v6 gate](data/native-prerender-combined-v6-failed.zip).
SHA-256: `764d8edfc7091c7ba4f9faa91f7a62578613c1026b685988fc9672c18b72d5a4`.
All 23 members were read back byte for byte. The manifest binds actual
receipts, stdout/stderr, raw performance samples, original strict assessments,
MCP evidence and bounded native diagnostic records to their hashes. Private
executables, symbols, raw Windows reports and later reruns are excluded.
