# Issue 85: rational clocks with merged native tracking replacement

The clock and parameter-animation branches were aligned to main
`0790404fab0328c99ffcc525ca4b56084a383951`, including the merged paired
source/solve transaction. Frozen child source
`6135221460639e18450e91d395e5459648049391` passed the feature-enabled host/desktop
all-target check and 17 distinct scoped tests. This is integration evidence,
not a full desktop/workspace run or completion of issues 85, 86 or 88.

The original combined check failed with two `E0277` errors: native video
fixtures still used `u64::from(project.fps)`. Both initializers now call
`FrameRate::frame_at_tick` with a microsecond timebase and explicit `Floor`
rounding, preserving the original positive-duration boundary. Checked integer
conversions replace the previous cast. All source outside those two
initializers, including every assertion, was compared and retained unchanged.
No production panel, layout, style, return value or budget changed.

| Executed scope | Distinct passes | Boundary |
| --- | --- | --- |
| Complete core `tracking_replacement` target | 8 | No ignored or filtered tests |
| Host paired-publication module | 5 | 171 unrelated tests filtered |
| Native linked-audio replacement | 1 | Original picture/audio, rollback, undo/redo and reopen assertions |
| Native paired source/solve replacement | 1 | Original publication, refusal, rollback and history assertions |
| Native identical/changed-source replacement | 1 | Original source-identity checks |
| Native mapped original video | 1 | Stretch, reverse, source remap and hold; original PTS, camera, history and reopen checks |

Each successful native command selected its actual test rather than accepting
an empty filtered run. Four reference/reopened image pairs were saved, compared
pixel for pixel and visually reviewed as a labelled contact sheet. Actual
paired before/after and linked-audio plate captures were also reviewed. These
are decoded fixture pictures rendered on the GPU, not physical screen capture,
real-footage quality acceptance or a fractional-project-rate matrix.

Two harness failures are retained alongside the initial compilation failure:

- The first native invocation failed in the Windows loader with `0xc0000135`
  before any test executed. Five existing FFmpeg runtime DLLs were copied to
  the rebuildable E-drive test directory and verified against their source
  hashes. The unchanged test executable then listed the two selected tests.
  No DLL bytes or operating-system loader policy changed; the underlying reason
  PATH lookup was insufficient is not established by this experiment.
- The combined observation invocation passed audio, then failed when the next
  test attempted to initialize a second process-global logger. The harness had
  enabled both optional output directories in one process. Only the remaining
  three tests subsequently ran, each in its own process. The already successful
  13 core/host tests and audio test were not repeated, and no logger or original
  test assertion was weakened to obtain a pass.

All owned controllers reached terminal status and their original sessions were
consumed. Source hashes remained unchanged during execution. The
[receipt](data/frame-rate85-tracking-integration-v4.json) and
[archive](data/frame-rate85-tracking-integration-v4.zip) retain all four run
records, the loader probe, exact before/after fixture source, original failed
outputs, successful commands, source closure, image pairs and a read-back
verified member manifest. Runtime DLLs themselves are not added to the repo.

The [historical green CI receipt](frame-rate85-86-green-ci-current.md) omits
FFmpeg by PR policy and is bound to earlier heads. Refreshed current-head CI,
remaining professional footage acceptance and the separately retained native
product stack/heap failures remain open. Android is excluded. No higher-level
tracking accuracy, arbitrary geometry antialiasing or complete 3D route is
claimed by these scoped passes.
