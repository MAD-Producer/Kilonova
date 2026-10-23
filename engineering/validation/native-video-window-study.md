# Real file video and native-window cost study

This explicit Windows Release driver supplements issue #60's offline six-case
baseline. Implementation and fixture preparation are available; a successful
Motion Studio collection and its source-bound evidence are required separately.
This document alone is not acceptance of the issue or of the native study.
Issue #60 is delivered for the maintained desktop scope. The user explicitly
excluded Android validation; no Android device result is claimed.

## Running

Use existing, explicitly selected FFmpeg/ffprobe tools to author four independent
AVC files. Source pixels are independently authored backgrounds, rectangles and
time-dependent rotation. No external creative assets or new dependency is used.
Commit and freeze the exact source before either command. Both output folders
must be new. `FFMPEG_DIR`, the matching shared runtime on `PATH`, the existing
compiler toolchain, and writable temporary/build locations must be configured.

```powershell
python tools/generate_window_video_fixtures.py --output E:/study-fixtures --ffmpeg E:/tools/ffmpeg.exe --ffprobe E:/tools/ffprobe.exe
python tools/collect_native_video_window.py --fixtures E:/study-fixtures --output E:/window-study --target-dir E:/study-target --environment-note "Describe background load, power and thermal conditions"
python tools/collect_native_video_window.py --output E:/window-study --validate-only
python tools/native_frame_activity.py --study E:/window-study --output E:/window-activity.json
```

The collector builds the actual `motion-studio` test binary with the repository
Release profile and `ffmpeg`, then explicitly selects the ignored native study.
It is compiled only for Windows tests with `ffmpeg`. Ordinary `cargo test` does
not execute the expensive native study. The separate manifest rejection test
is cheap. Existing assertions and test execution policies are unchanged.
The tool unit suite also checks partial/missing GPU sample rejection and actual
failed/nonreturning child receipts. Its synthetic profile rows are protocol
fixtures, not device measurements or performance acceptance. The runner embeds
the frozen revision in the Rust test binary through a tracked compile-time
environment value and requires it to match the final evidence revision.
Validation reads the primary host report at
`project/exports/preview-performance.json`. The retained `native-profile.json`
copy now preserves its bytes. Earlier driver copies could change fractional
frames by one ULP during JSON parsing/serialization; those failed collector
results remain unchanged. Strict source-frame and GPU-distribution equality
still applies to the original report, with no tolerance or rounded frame IDs.

## Fixed workload and interval

- Four distinct source files, each3840x2160,30fps,600frames/20seconds, AVC SDR
  limited-range BT.709; retain actual probes, PTS indices, file and tool hashes
- Compare one full-frame layer with four independent layers/asset IDs/files in
  quadrants. These are different workloads, not optimization before/after
- Composition1280x720, surface1280x800, explicit High preview and256MiB source
  texture allowance. Record the full memory policy, actual quality and source
  formats; a resolution downgrade or resource refusal fails this fixed study
- Record the first render of each fresh Session/device separately. OS file and
  driver caches are not cleared, so this is not a system-wide cold-cache claim
- Start production Shell playback at source frame0. Admit frames using its
  actual wall clock for four seconds: one second of warmup and three measured
  seconds. Complete the last admitted render even if it crosses the boundary
- Preserve fractional source times, repeated source buckets, skipped source
  buckets, pending polls and each actual completion. Do not require every
  arbitrary driver call to advance exactly one frame

## Observations and limits

The timed render slice includes seek, UI construction and native Present replies.
Explicit state/resource queries and evidence IO occur afterward, with their
costs retained separately. Completion intervals and playback pacing include
those costs. The driver pumps native window messages through the existing test
helper; it does not dispatch ordinary application input or measure physical
input responsiveness. The windows are visible and use actual native surfaces.

The existing host FrameRecorder supplies frame-sequence-matched CPU/native and
supported root composition/presentation GPU timestamps. Missing samples remain
missing, not zero. These timestamps exclude earlier conversion/nested/vector/
MSAA GPU stages. Their full coverage remains a separate #60 requirement.
The recorder matches actual native sequence numbers, including gaps caused by
chrome-only presentations during decoder polling. The evidence verifier checks
raw sample counts, coverage and distributions. A timestamp-capable device with
zero associated samples fails; unsupported hardware keeps null timings.

The source's next nominal frame boundary is compared with the native reply
completion time. This is a requested-source deadline observation, not a display
compositor, monitor scanout, PresentMon or physical display FPS measurement.

After profiling stops, an explicit observation reads real prepared native source
frames and records actual dimensions, pixel layout/capacity, decoder identity
and PTS. This final operation can change request/prefetch bookkeeping and is
excluded from sampling. A native UI capture is also excluded. Its video centers
are compared with actual independently decoded RGBA files, with a fixed8level
RGB tolerance; reference centers must be identical in all three retained source
samples. Pixel checks cannot be replaced with successful API returns.
Fixture generation checks those actual centers before publishing its manifest.
The first authored background uses black; the composition background is magenta
so a missing source cannot accidentally match the black reference. Colored and
neutral-gray first-background attempts had different actual first/later center
bytes and remain failed evidence. The strict native center
equality and output RGB tolerance remain unchanged; the original failed run
must remain separately attributed to its original source and files.

Every potentially blocking phase has flushed start evidence. Native reply
failures retain a nonblocking worker/native snapshot and the original cause
before Engine Drop. The outer collector bounds only its own explicit test
process to600seconds; it does not add product timeouts, retries or restarts.
The original build/native stdout, stderr, process IDs and exit receipts remain
available even after failure. A successful build is not native acceptance.

The CPU/texture inventories report observed ownership/capacity, not physical
VRAM or process RSS. Decoder external allocations, system pressure,
complete GPU-stage coverage and controlled optimization benefits
remain outside this increment and must not be inferred from these records.

## Completed-frame interval activity

The optional postprocessor first revalidates the complete original native study,
including actual binary identity, source files, decoder streams, output pixels
and GPU sample coverage. Failed studies cannot produce an activity report.
The input records are never rewritten; output must be a new file. Input hashes
and the original measured revision are carried into the derived report.

Each row retains cumulative counters and their differences from the preceding
successful composition completion. It includes instrumented submissions, video
uploads/conversions, image/video/vector upload payload bytes, effect pipeline
builds and compositing pipeline-set builds. Counts include intervening decoder
polls and chrome presentations. Overlapping conversion counters must not be
summed. These are observations in the same attached renderer, not exclusive
per-command costs, physical queue submissions or measured bus traffic.

The cold row has no earlier baseline and keeps null intervals. Missing or
decreasing counters also keep null intervals, with explicit reasons; later
valid observations establish a new baseline. Measured P50/P95/P99 distributions
exclude unknown values and report their counts separately. Valid zero activity
is retained. The pipeline counters do not cover all startup/driver compilation.
Returned timing payload bytes describe associated timestamp samples only.
Image readback bytes remain unknown. Full GPU-stage and readback coverage still
require separate instrumentation and acceptance.
