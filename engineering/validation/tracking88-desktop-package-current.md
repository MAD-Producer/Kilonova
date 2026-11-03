# #88 complete FFmpeg desktop package regression

The complete `motion-desktop` package passed on combined source
`52ce7ebf0571b12ae2bea7eb9352493165955a7e`, including #583's transaction and
native subprocess diagnostics and #584's MCP workflow. All 1772 recorded Rust
and manifest/lockfile hashes stayed unchanged during the run. The original
validator exited with code 0; there is no live validator for this receipt.

```text
cargo test -p motion-desktop --features ffmpeg --locked --offline --no-fail-fast -- --test-threads=1
```

| Target | Passed | Failed | Existing ignored |
| --- | ---: | ---: | ---: |
| motion-studio binary | 1529 | 0 | 50 |
| dock_frame_cache | 4 | 0 | 0 |
| effect_search_cache | 2 | 0 | 0 |
| effect_track_cache | 2 | 0 | 0 |
| timeline_row_cache | 3 | 0 | 0 |
| timeline_scroll_signature | 2 | 0 | 0 |
| Total | 1542 | 0 | 50 |

All five package integration targets are confirmed in Cargo's original output.
The binary test phase took 3165.20 seconds; the controller's total, including
compilation and final source verification, was 3394.203 seconds. Existing
explicitly ignored tests were not selected with `--include-ignored`; their exact
names and reasons remain in the receipt and stdout. No new ignore annotation,
retry, relaxed assertion or machine/driver change was used for this result.

The [source-bound receipt](data/tracking88-desktop-package-v1.json) and
[source/controller/log archive](data/tracking88-desktop-package-v1.zip) retain
all six summaries, exact command, source identities, raw streams, the precision
child identity and controlled precision captures. The archive has 109 members,
7,443,515 bytes, and every member was read back and compared before publication.
Unchanged source is identified by the tested Git commit and full hash manifest;
changed transaction/MCP/test sources and all five integration sources are also
included in the archive.

## Earlier failure remains part of the evidence

The previous complete **binary-only** run failed three subprocess wrappers:
mask admission, text-style export/reopen, and project precision. All three pass
inside this new complete package invocation, including all three native tracked
video controls. The precision child receipt records PID 37012, exit code 0,
`0x00000000`, and elapsed time 98005 ms.

This is a new-source successful regression, not an unchanged reproduction of the
earlier failures or proof that their access-violation root cause was repaired.
The test-only child diagnostics preserve evidence; they do not alter production
rendering, fix a driver or explain the earlier unknown-module Windows events.
The earlier failed receipt and isolated passing controls remain retained in the
[paired replacement record](tracking88-paired-replacement-current.md).

## Scope

This is a Windows FFmpeg-enabled **desktop package** gate. It is not a complete
maintained-workspace run, qualification of every existing ignored manual study,
or an exact-head cross-platform CI claim. The snapshot precedes the separately
added actual-Surface regression; that new test needs its own source-bound result.
No inference about retained interactive tracking fronts or real-footage tracker
quality is made from these offscreen tracking controls. #582's photographed
quality failure and the rest of #88 remain open. Android is retired and excluded.

No panel, UI interaction, layout, style, dependency or third-party asset changed
to obtain this result. No cache, foreign worktree or earlier evidence was deleted.
