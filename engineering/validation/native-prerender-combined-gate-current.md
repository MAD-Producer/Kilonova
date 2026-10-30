# Combined native prerender gate: failed

Issue: [82](https://github.com/MAD-Producer/Kilonova/issues/82).
Draft: [PR 575](https://github.com/MAD-Producer/Kilonova/pull/575).
Exact tested source: `510484160fde38d18b177ba8c69baf19af1730dc`, rebased onto
main `49890ce0da7d7701eaec5e521d749b562d3ffd47`.
Completed: `2026-10-30T17:05:54.199018+00:00`.

This combined gate failed. The later documentation commits and queued source
components were not part of the tested source. Passing individual controls or
the five required PR CI checks does not override the failed local gates.
Android is retired and excluded by user direction.

## Actual results

The gate executed the existing scheduler, native producer, Engine input
priority, source replacement, revision refresh, static-window, playback,
pressure, blocked-worker, idle and four-precision upload controls. It also ran
the unchanged independent evidence parsers, the actual 480-sample native input
study, the actual 240-frame paced playback study, the maintained workspace,
desktop check/build, shared native-window MCP, provenance and tool inventory.
Exact commands, source hashes and per-step results are in the attached receipt.

Both strict performance assessments failed:

| Study | Failing independent pair ratio | Disposition |
| --- | --- | --- |
| Timeline input, second pair | `1.0165369097005172` | P95 regressed; pooled improvement cannot override it |
| Paced playback, first pair | `1.0768093078288306` | P95 regressed; 58/59 warm cache hits do not prove latency acceptance |

The maintained workspace command was:

```text
cargo test --workspace --exclude motion-android --features motion-desktop/ffmpeg,motion-host/ai-depth --locked --offline --no-fail-fast -- --test-threads=1
```

It exited with code 101 and two failed targets. Completed test summaries report
3,973 passed, one failed and 60 ignored. In addition, `pointwise_fusion` exited
abnormally without a final summary. Its unreported tests are not passes. The
desktop unit target's 1,461 passes and 51 existing ignored tests are a subset of
the workspace results, not an additional total. Ignored controls retain their
original selection and are not counted as passes.

Desktop check, video-enabled debug build, actual shared-window MCP, provenance
and tool inventory completed successfully after the workspace failure. The
controller deliberately collected these independent results without changing
the overall failed disposition. The current PR head's five required CI checks
also passed, separately from this local result.

## Offline metadata failure and environment correction

`motion-model/tests/dependency_boundaries.rs` failed when its unchanged
`cargo metadata --locked --offline` child could not find the locked
`alsa v0.9.1` archive. It exited before evaluating dependency-boundary assertions.
Successful compilation on Windows did not establish a complete all-target
metadata cache, even with existing unpacked source directories available.

The repair populated 215 absent official `static.crates.io` archives in the
task-owned E-drive Cargo cache. Every download matched the existing Cargo.lock
checksum before publication. Independent read-back verified all 576 locked
registry archive checksums, including those present before repair. No new
dependency, lock, source, test assertion or C-drive cache was modified. The
repair started no additional Cargo/GPU validator and extracted no archive.

This is an environment correction, not a passing test result. The original
failed test remains failed in the receipt. Its unchanged assertions must run in
the final integrated regression using the corrected cache; no unchanged full
suite was retried for this repair.

## Native abnormal exit

Cargo reports `pointwise_fusion-e5d71194cb7bcfc4.exe` exiting with
`0xc0000005 / STATUS_ACCESS_VIOLATION`. The read-only monitor preserved its
original EXE and PDB before another build could overwrite them and verified both
copies. The original Windows report was separately preserved and matched to
the exact executable path. It identifies `StackHash_b0ae`, exception data `8`,
and an unknown fault offset. No original dump or exception context was recovered.

The preserved EXE SHA-256 is
`2765985fefaf6d28461c3e034409827d65ad60db96c7e129a950018abd8006de`;
the PDB SHA-256 is
`5e5c468b70768ec930bb4e5d07704eb21d7a98f307fdb50cf30a7c1e3bac2ab8`.
The report identifier is `9710971d-2c29-4a85-a0c9-79db6a05dc72`.
Graphics and overlay module presence does not establish causality, and the last
printed test name does not establish the faulting thread or phase. Root cause
remains unresolved. The earlier native abnormal exits and failed performance
studies also remain failed; they are not silently superseded by later passes.

## Remaining work

Three independent components require their own genuine negative/fixed
controls: terminal source-watch faults after ordinary changes; avoiding an empty
OS watcher for procedural sources; and removing foreground installation's
unconditional Info/flush while batching its native GPU upload. Their nine new
tests preserve all existing test lines. A scratch-copy application proved only
textual integration; these changes are not accepted by this source's results.

Final acceptance still requires actual combined-source input/playback P95,
complete maintained regression, native failure investigation/disposition and
the remaining original issue requirements, including explicit optional disk
cache and quota/full/corruption qualification. The current memory-only profile
does not prove those disk-cache requirements. PR 575 remains a draft and issue
82 remains open. The independently completed main-source issue87 release gate
is documented [separately](scene3d-main-release-current.md).

## Evidence

Archive: [failed combined gate](data/native-prerender-combined-v5-failed.zip).
SHA-256: `92ef9c0a83774b134bdc3984a3370075fd8f35de4bd5d299090907c4a37fdaf3`.
All 18 members were read back byte for byte. It includes the actual terminal
receipt, workspace logs, strict assessments, desktop/MCP outputs, locked-cache
repair/read-back records and a bounded native abnormal-exit disposition.
The preserved EXE/PDB, raw Windows report and any later diagnostics remain
private forensic artifacts; they are not included in the public evidence ZIP.
