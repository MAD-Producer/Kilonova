# #88 nonlinear video, layer clocks and current-main prefetch

PR #586 synchronizes parent #584 `8dafc3188bdf53d50ce61a6872bf2cbc78fbfc99` and freshly
fetched main `e23475585ce598dd4747342c4b0a3face71add50` in one conflict-free merge. This includes
main's newly merged mapped-video-prefetch fix. No new product implementation,
test assertion, UI, dependency or asset is authored by this synchronization.

The complete model, core and render crate trees exactly match the earlier
layer-time integration `18579d81`; their Git tree IDs are recorded. That earlier
model/core full result stays historical. The host now includes main's prefetch
changes and is actually revalidated below, alongside the combined desktop paths.

## Executed gates on frozen current source

- render: 100 passed, 0 failed, 0 ignored; 0 filtered
- host: 176 passed, 0 failed, 0 ignored; 0 filtered
- surface: 2 passed, 0 failed, 0 ignored; 1752 filtered
- prefetch: 2 passed, 0 failed, 0 ignored; 1752 filtered
- mapped: 1 passed, 0 failed, 0 ignored; 1753 filtered

Seven complete renderer targets cover nonlinear tracking lenses, RGBA/YUV,
queued-video lifetimes/instances/allocation, model layers and mapped nested time.
All eight independently ray-checked queued-video outputs retain the original
stable-pixel/edge policy and two-byte threshold; the measured maximum is 1.

The two real Windows Surface controls keep a native window/device attached while
checking camera/video and fixed-plate point/plane/nonlinear-lens replacement,
committed-front pixels, actual Present, invalidation/reuse, undo/redo and reopen.
The four-case original-video clock control separately compares stretch, reverse,
linear source remap and hold to native references through replacement/relink and
history. These are separate bounded controls, not every possible combination.

Both current-main prefetch controls execute through the real FFmpeg decoder.
They compare CFR/VFR pixels under authored clocks and check actual planning
failure admission, continued required-video progress, suppression and recovery.
With MOTION_LOG unset, the injected planning failure logs Error at count 60,
suppresses repeats, resets on recovery, and logs once at the next count 60. Both
Error records appear in file and stderr, with neither in child stdout. The
existing test assertions are retained; 60 events are not labeled a duration.

The [receipt](data/tracking88-lens-clock-current-v1.json) and
[read-back verified archive](data/tracking88-lens-clock-current-v1.zip) retain
all commands, source identities, relevant staged-LF snapshots, native images,
independent oracle reports, process exits and original logs. Source remained
unchanged throughout execution. No panel differs from maintained main.

## Retained compiler failure and scoped recovery

The first invocation completed the seven renderer targets and complete host
library, then rustc exited with 0xc0000005 while compiling motion-render for the
desktop feature set. No Surface test executed in that failed step. Windows
Application Event 1000 names rustc_driver-e41b336de1bfe42c.dll, offset 0x180bd40,
with the matching compiler failure. The original failure, source hashes and event
remain archived. This identifies the compiler phase, not the underlying cause.

The next invocation checks identical source hashes, retains the already-passed
renderer/host steps without repeating them, and reruns only the unexecuted
Surface, prefetch and footage-clock groups. No Rust source, flags, assertion or
physical/image threshold is changed. The aggregate above therefore spans the
two source-identical invocations; it is not a single all-green original run or
a claim that the compiler or earlier product access violations are repaired.

## Remaining acceptance

This result totals 281 passing tests in the specified commands. It is not a
fresh complete desktop package or maintained-workspace regression. The previous
full desktop result and old-head green CI remain historical. Final-head CI,
earlier unexplained native crashes, optional photographed tracker quality,
prerequisites #85/#86 and full #88/3D-route acceptance remain separate work.
The hidden Surface does not establish physical display scanout. Android remains
retired and excluded. No PR merge or issue closure is implied.
