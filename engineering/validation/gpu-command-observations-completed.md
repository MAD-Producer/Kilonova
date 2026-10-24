# GPU command observations: accepted correctness

Issue #60, maintained desktop scope. This record describes the command-observation
increment. The issue-level requirement audit and final PR checks are separate.
Android/device acceptance is excluded by the user's confirmed scope.

## Measured and tested source

All current results below belong to
`ff52fbd2abd370b1d72fa6c86231cf8f4d24fa21`, based on main
`d07b8b13a493892ec75e35945d728c08e5527446`. They do not relabel predecessor
measurements. PR #466's resource-pressure/readback results keep their original
sources and limits.

The complete ffmpeg Desktop regression actually passed **893 tests, zero failed,
three existing ignored**: 880 in the main binary and 13 integration tests.
Default and ffmpeg Desktop checks passed. Focused execution additionally passed
48 Render/Host/Desktop tests, two example tests, 17 Python protocol/archive
tests and 15 affirmative structured native reports. These overlapping test
selections are not added to the full-suite count. Full Render/Host 703/0/1
belongs to the earlier `11c342d3` source; it is not claimed as a complete ff52 run.

Six actual Debug correctness cases retained 54 frames and 99 native command
records. Independent PNG decoding checked every final RGBA hash and dimension.
Debug samples establish protocol/pixel consistency, not Release latency or an
independent intended-scene visual oracle. Release results are published separately.

## New observations and actual failure evidence

| Item | Behavior and actual evidence | Limit |
| --- | --- | --- |
| G1: state before encoding | Publish Encoding before native encoder creation; controlled blocking verifies live state before completion | Explicit profiling only; no claim to observe every raw GPU caller |
| G2: bounded identities/state | Nonwrapping epoch/command IDs, 128 active commands, 256 recent records, 64 contexts and bounded causes; exhaustion/omission remains visible | Allocation-window contexts are shared active windows, not exclusive ownership |
| G3: native retirement | Original query words, native mapping, actual pixels, cancellation, submission unwind and actual queue completion; uncertain submissions quarantine slots | No speculative retry/restart or reassignment of old callbacks |
| G4: opt-in integration | Renderer and independent input owners preserve their devices, admission and original raw/allocated submission contracts | Default rendering allocates no command query pool or markers |
| G5: exact request identity | Exposure requests/polls and input packet identities survive pending work, restart and supersession | Offline benchmark sample IDs are explicitly distinct from Engine worker requests |
| G6: exact presentation links | Four successful native normal/exposure surface presentations join exact epoch/encoder IDs and original words; 41 retained command records | No scanout, intended-scene pixel oracle or steady-state playback claim |
| G7: trace recording | Actual 266 native samples retain a 260-record window across ring eviction, excluding two earlier and four later commands; actual production caps are tested | 8192 commands / 16 MiB retained metadata; this bound excludes allocator/copy overhead |
| G8: default failure sinks | Two default-Info child cases exercise the real logger and Session guards, file/Error stderr, stdout isolation and preservation of the primary cause | Metadata invariant and native invalid-word failures are distinct paths |
| G9: threshold/recovery | 130 actual invalid diagnostic readbacks, two valid native samples and 64 recovery pixels prove 59 quiet, occurrence-60 Error, suppression through 70, valid reset and a second Error episode | Deliberate GPU diagnostic-buffer corruption, not an actual driver/hardware fault |
| G10: collector protocol | Original timestamp words/queue period, exact main IDs, completed allocation windows, per-frame diagnostic bytes and cold/warm/steady distributions | Root and encoder intervals may overlap; never sum them into exclusive GPU busy time |

The diagnostic injector is gated by `debug_assertions` and `gpu-tests`, absent
from Release and never invoked by ordinary rendering or profiling startup. It
uses a real GPU copy to corrupt diagnostic query words; the unchanged native
callback, counter and logger process the failure. No successful callback is
fabricated and no image/project data is modified.

Callbacks retain bounded events without file/console IO. The outer log scope
flushes after releasing observation and Session guards, including unwind.
The first failure is Debug; occurrence 60 is Error once per consecutive episode.
Success or explicit cancellation resets the episode. Omissions/discarded work
do not prove recovery. Counts are occurrences, not elapsed-time estimates.

Command `elapsedUs` is wall time through encoding, queueing and mapping;
allocation-frame `cpuElapsedUs` describes its separate CPU scope. Neither is
exclusive CPU or GPU busy time. Ordinary Present logging remains quiet.

## Retained failures

Historical failed attempts remain failed in the archive. Source `11c342d3`
passed full Render/Host 703/0/1 but failed full Desktop 877/1/3 in an existing
composition-bundle child. The unchanged child later passed once in isolation.
Contemporaneous Windows access-violation events name the same test executable,
but exact child association/root cause remain unestablished. ff52's later full
pass does not establish a fix for that earlier native crash.

Source `9c48a419` failed Desktop compilation because `gpu-tests` was not forwarded
to the dependency. `aa06d1dd` fixed feature forwarding and passed native sinks/
presentation tests, then failed a render unit fixture missing `period_ns`.
ff52 supplies the fixture conversion constant without changing its assertions.
The first ff52 integrated run stopped at the disk guard before later phases;
the accepted integrated-v3 execution actually completed them. No failed receipt
is overwritten, and no earlier successful subset substitutes for the later run.

## Published evidence

The [raw correctness archive](data/gpu-command-correctness-ff52fbd2.zip) contains
299 entries: 298 payload files independently checked by length/SHA-256 and an
inventory. It retains original streams/start/exit receipts, source/binary
identities, native reports, projects/PNGs, independent pixel verification and
historical failure dispositions. Its source bundle requires the recorded main
prerequisites; tracked input hashes and the binary patch are included.

Archive SHA-256:
`7efd16ee13bf318d0890b1b881663a2e8b6f383ba3b129b6135654163cf8b14c`.
The [archive manifest](gpu-command-correctness-acceptance.json) records exact
source, size and independently verified entry count. No executable is packaged.

## Product and acceptance boundaries

No dependency, third-party asset, panel, token, layout, style or interaction
changes. Existing DESIGN remediation rows are retained; no UI row is claimed
fixed. Geometry and generated pixels are independently authored. Normal
rendering does not wait for diagnostics or enable per-frame tracing. This
instrumentation is neither a rendering optimization nor a universal profiler.

Other adapters/backends, physical display deadlines, driver residency, physical
OS exhaustion and comparative acceleration are not established by these results.
Complete GPU coverage remains false. Final-head PR CI is required before merge.
