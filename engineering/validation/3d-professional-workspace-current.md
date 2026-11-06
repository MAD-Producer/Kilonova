# Combined 3D and professional-compositing workspace regression

## Current integration: complete execution and matching CI

Source `c190ab209abdefce4511486f1f51020599312e8a`, including main
`50bef478120a8f5295773042277b90c68d7f76ba`, passed the exact complete-workspace
command below: **4,896 passed, zero failed, zero filtered, 78 original ignores**.
All 417 target summaries and invocations match Cargo metadata. Original session
14345 exited zero and was consumed; source hashes and HEAD remained unchanged.
Execution took 7,024.281 seconds, 10:59:06 through 12:56:10 UTC on 2026-11-05.
The desktop binary's 1,753 passes and six passing integration targets are
included, not added to that total. The source comparison retains all 14 changed
source files, no removed targets and three new targets since the older pass:
`layer_time_frozen_export`, `panel_paint_pixels` and `panel_paint_reuse`.

[Matching-source CI run 42015343857](https://github.com/MAD-Producer/Kilonova/actions/runs/42015343857)
completed successfully: audit, Rust/GPU, three desktop test platforms and three
builds (Windows, Ubuntu and Intel macOS). All eight complete logs and run/job
metadata were retrieved and hashed. Each of 17 key/screen, child-protocol,
parameter-animation and strict-clock controls passed exactly once on each
desktop platform. Windows also passed both native rational-clock Surface
controls. PR-policy skips for libav and publication are not runtime passes.

After the workspace process terminated, the unchanged private depth probe was
rebuilt with `--locked --offline` against that exact product source. Actual
CPU ONNX inference, authored indexed-video calibration, frozen PNG export and
the independent f64 ray/triangle oracle passed in 176.172 seconds. Product and
probe hashes remained unchanged. The original 76,667-us request selects indexed
PTS 50,000; prepared 52-by-80 RGB matches its retained fixture exactly.
Normalized maximum/mean error against the pinned historical PyTorch reference
is `7.787024532934269e-7` / `1.2020595162418164e-7`, within unchanged limits.
The original oracle checks 27,884 far and 330 near pixels, maximum RGB error
one byte; its 70,090 excluded pixels and qualification remain explicit.
Both PNGs were visually inspected and export after live Estimate/Session
closure while the original source remains available. Missing-index failure,
default-Info start, file/Error-stderr output and stdout isolation also pass.

The [current archive](data/3d-professional-workspace-c190ab20.zip) and
[manifest](data/3d-professional-workspace-c190ab20.json) preserve all local output,
current CI, the actual inference probe/outputs and 33 source-bound named controls.
All 56 members were read back and hash-verified. Archive: 1,598,154 bytes;
SHA-256 `9f855e55c08eeda47711f926efb18886901782b1b49e0c30f3dc24495cb32b45`.
No runtime, executable, weights, installed cache or new third-party media is
included. The generated video fixture is independently repository-authored.

Assertions, ignores, budgets, stack, GPU backend and default Info were retained;
cache/temp/target remained on E. Android is excluded by the user's scope.
This is bounded SDR/model evidence, not physical accuracy, HDR or arbitrary
footage acceptance. Historical natural-domain/reference records keep their own
scope; no new PyTorch/domain matrix is claimed. Historical native crashes did
not recur; this does not establish a diagnosed repair. The optional tracker's
real-sequence quality remains failed. Issues 74/85/86/88 were open when this
record was written; the entire 3D route is not declared complete.

## Historical source-bound execution

The complete maintained Windows workspace passed on
`84c1776016a3d23193493097eecb27eb2c418895`, based on main
`e314cc92232dcd366b01e10454ae1100d4e24d63`. This source combines the external
camera/lens route with the key, manual-holdout and photographed-screen samples.
The original controller session 4254 exited zero and was consumed.

## Actual complete execution

```text
cargo test --workspace --exclude motion-android --features motion-desktop/ffmpeg,motion-host/ai-depth,motion-desktop-media/ai-depth --no-fail-fast --locked -- --test-threads=1
```

The result is **4,885 passed, zero failed, zero filtered, 78 existing ignored**.
All 414 target summaries and actual invocations match Cargo metadata for the
eleven maintained packages, including their documentation tests. The original
assertions, ignores, quality thresholds, stack and GPU backend were retained.
No failed target was removed from this command. Android is excluded by the
user's maintenance scope.

Compilation and execution took 8,332.766 seconds, from 08:25:58 to 10:44:51 UTC
on 2026-11-05. All tracked source hashes and HEAD matched the frozen initial
checkout at completion. SDK sources were materialized before the run. The
dedicated target, temporary files and controlled cache were on E; the user
performed the disk cleanup. Default Info, ordinary test/development codegen,
debug information disabled for this local build, one build job and serial
tests were used. No other own GPU or CV experiment ran concurrently.

This gate includes actual native FFmpeg movie/depth controls, public parameter
and rational-clock commands, history/reopen, imported camera/video rendering,
all three professional sample targets and the UI framework tests. Enabling the
optional inference feature does not imply that every external-model study or
original manually ignored acceptance case executed. The separate actual-model
[inference-to-frozen-output record](depth74-inferred-render-current.md) retains
its own source, runtime, model, reference and numerical evidence.

The earlier access violations and disk/link failures remain in their original
records. They did not recur in this complete execution; this result establishes
a pass for the recorded source, not a diagnosed repair of those failures.

## Exact prior remote CI results

| PR/source | Terminal result | Scope |
| --- | --- | --- |
| #606, `5c6c654b`, run 41988958264 | Success | Audit, Rust/GPU, three desktop test platforms and three desktop builds all succeeded |
| #612, `b1643aa1`, run 41991436758 | Cancelled | Audit, three desktop test platforms and three builds succeeded; Rust/GPU exceeded its original 45-minute limit |

All completed job logs, run/job metadata and timeout annotations were retrieved
and hashed. Both original key tests and the new photographed-screen test appear
exactly once as passed in each Windows, Ubuntu and Intel macOS desktop test log
for `b1643aa1`. Its Rust cancellation is not counted as success. The combined
branch already contains the 60-minute Rust/GPU budget from the camera branch.
PR policy skips FFmpeg/libav runtime acceptance and publication; these old-head
results are not transferred to the combined source.

## Integration and issue boundaries

Main advanced to `50bef478120a8f5295773042277b90c68d7f76ba` while this gate ran,
adding paint-buffer changes and layer-time/vector-upload tests. This receipt
does not establish execution of those later sources. Aligning the combined
branch and validating that integration remain separate delivery work.

Merge `b6e3ffc3` subsequently aligned the branch with that main source without
conflicts. The provenance and panel-style guards passed; the branch has no
panel changes relative to the new main. A fresh complete maintained-workspace
execution and combined-source CI are required for this integration. The
4,885-pass receipt above remains attached to its original source.

Issues 74, 85, 86 and 88 remain open. The separate optional tracker's failed
real-sequence quality gate is not repaired by successful imported-data rendering.
The key's unprotected dark-foreground failure, manual alignment of the screen,
separate calibrated camera poses and explicit SDR PNG output retain the limits
described in their individual records. Neither this gate nor its count certifies
arbitrary footage, continuous 3D camera solving, HDR files or the entire 3D route.

## Portable original evidence

The [archive](data/3d-professional-workspace-84c17760.zip) and
[manifest](data/3d-professional-workspace-84c17760.json) preserve the full
controller, Cargo metadata, stdout/stderr, terminal source/coverage receipt,
both prior remote runs and their log-retrieval/coverage observers. All 34 members
were read back and verified against original bytes and SHA-256.

Archive: 1,318,504 bytes; SHA-256
`92be4d551d68ea852252c9cb351a136778ddf64c5350a571738da74fbef6633c`.
No executable, runtime, weights or new third-party media is included.
