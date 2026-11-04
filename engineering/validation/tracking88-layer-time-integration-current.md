# #88 integration with the maintained layer clocks

This increment integrates main `9d085c0f1fb63106babb48d5781b700e20da6eac`
with tracking/video-lens head `8672e59e8492bb654ac39c5f5777f0f228144253`.
Main introduced continuous layer stretch/reverse, source remapping and local
key addressing after the earlier tracking source was frozen. Results on the
older source are historical, not acceptance of this combined source.

## Merge decisions

Both the tracking commands and `LayerTime` remain registered and accounted for
in history. Transaction routing retains main's continuous gesture edits and
the tracking branch's atomic picture/solve replacement. Neither feature is
discarded to resolve the textual conflicts. The project schema is now **36**:
the combined document differs from main's 32 and tracking's 35. This follows
the repository's current-version-only policy; no migration was introduced.

No panel code was authored or changed relative to the integrated main. Panel
changes inherited from main remain main's existing work, not a new UI design.

## Added combination controls

- Point displacement and authored correction keys share the same fractional
  local time under 3/2 stretch, forward/reverse layer direction and either sign
  of the binding speed. The original PTS and binding metadata remain unchanged
- Plane observations and authored corner corrections share that local clock
- A tracked camera layer keeps its pose, calibrated projection and manual
  camera corrections on one local clock
- A layer-clock edit and an atomic picture/solve replacement retain separate
  undo/redo steps, unchanged clock data and a current-schema JSON roundtrip

These controls compare mapped sampling to the existing unretimed source
fixtures at explicitly tabulated local instants, including fractional frames.
They do not substitute nominal video fps for the imported original PTS.

The bindings retain their documented **target local property clock** semantics.
A source-only remap on a different footage layer does not automatically retime
an independently bound null or camera. This increment does not add an implicit
source-layer association, decoder-frame snapping, or automatic reacquisition.

## Validation record

The complete model/core package rerun passed **859 tests, zero failed**, with
one pre-existing ignored manual history measurement, across 137 target/doc-test
summaries in 73.640 seconds. All source hashes stayed unchanged. The four new
combination controls passed. See the [receipt](data/tracking88-main-time-v1.json)
and [source and raw log archive](data/tracking88-main-time-v1.zip).

The first controller disappeared together with its
native process before a completion receipt was written; it is not counted as
a pass. The second invocation found a return-type error in the newly added
plane test (fixed array versus Vec). Only that conversion was corrected; no
assertion was changed. The third invocation completed with one failing new
test: its four-sample video fixture incorrectly occupied the entire five-second
composition, violating the existing recoverable-source range. Only the new
fixture's visible interval was corrected to four frames. All earlier failures,
source snapshots and raw compiler/test output remain in the archive.

The tools run initially used the system Python without PyYAML and failed two
test-module imports. Running with the existing repository validation Python
completed 110 tests: 109 passed and the existing optional native effect-loader
test skipped without `MOTION_EFFECT_VALIDATOR`. No new dependency was added.

Seven complete renderer integration targets on that same frozen source passed
**100 tests, zero failed or ignored**, including `layer_time_nested`,
`model_layers`, `tracking_lens`, `video_allocation_budget`, `video_instances`,
`video_lifetime` and `video_yuv`. The source-bound invocation took 165.453
seconds. All eight queued RGBA/YUV video/lens outputs retained maximum channel
error 1 against the independent ray oracle.

The actual Windows Surface group then passed **2 tests, zero failed or ignored**
in 15.67 seconds (112.031 seconds including rebuilding). It rechecks the point,
plane, nonlinear camera and previous camera/video replacement controls,
undo/redo/reopen, committed-front invalidation and normal subsequent reuse.
The remaining 1749 binary tests were filtered, not run. This is neither a new
full desktop package run nor a full maintained-workspace regression. The
[render/native receipt](data/tracking88-main-render-native-v1.json) and
[verified archive](data/tracking88-main-render-native-v1.zip) retain the raw
logs, source hashes, actual child identities, front images and first-run
interruption observation.

Final-head cross-platform CI remains separate. This is not completion of #85,
#86, #88, the failed photographed prototype or the 3D route.
