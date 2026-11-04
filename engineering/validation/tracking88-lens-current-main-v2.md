# Nonlinear native video: current-main integration

Source `1170ededa825a100d63131c2aa5255a2e55768a0` integrates parent #584 `52b26fda` and main
`0790404fab0328c99ffcc525ca4b56084a383951` after #583's Chinese squash merge. Synchronization
adds no new product behavior, test assertion, UI, dependency or external asset.
The existing nonlinear-video implementation remains the separately described
functional scope in [the original record](tracking88-native-video-lens-current.md).

## Actual frozen-source execution

The complete host/desktop FFmpeg all-target check passed before these tests.

| Command scope | Passed | Failed / ignored | Filtered |
| --- | ---: | --- | ---: |
| render | 100 | 0 / 0 | 0 |
| host | 176 | 0 / 0 | 0 |
| surface | 2 | 0 / 0 | 1787 |
| prefetch | 2 | 0 / 0 | 1787 |
| mapped | 1 | 0 / 0 | 1788 |

The seven renderer target names are matched individually against the actual
Cargo invocations and seven summaries. All tests in those targets and the host
library executed, without filtering. The total of 281 passes is restricted
to these commands; it is not a full desktop/workspace count. Existing assertions,
ignore markers, waits, pixel thresholds and allocation budgets were unchanged.

All eight independent queued-video ray checks exceed 250000 stable pixels;
their largest measured channel error is 1, within the original two-byte
threshold and original edge exclusions. The actual Windows Surface controls
retain an attached native device/window through fixed-plate camera, point,
plane and nonlinear-lens replacement, pixel invalidation/reuse, undo/redo and
reopening. New representative before/after captures are individually inspected.
This is native readback, not physical scanout or photographed tracker quality.

Two real FFmpeg prefetch tests cover CFR/VFR authored clocks and actual planning
failure admission/recovery. With MOTION_LOG unset, the failure episode emits
Error at count 60, suppresses repeats, resets on recovery and emits once at the
next count 60. Both Errors appear in file and stderr and are absent from stdout.
Sixty occurrences do not imply a fixed duration. The separate four-case native
original-video control checks stretch, reverse, linear remap and hold against
references through replacement, history and relinking.

The model/core/host/media trees exactly match parent `52b26fda`; their tree IDs
are recorded. Host is actually revalidated here. Historical full model/core
passes are not counted again, and the changed renderer is not claimed identical
to its parent. No panel differs from maintained main.

## Evidence and limits

The [receipt](data/tracking88-lens-current-main-v2.json) and
[archive](data/tracking88-lens-current-main-v2.zip) retain actual commands,
process exits, source identities, native reports/images, default-level failure
logs and a verified per-member manifest. Source hashes match throughout; the
original session 54025 exited zero and was consumed before publication.

The older compiler access violation remains retained by the preceding clock
integration record. The separate local clock-package allocation failure and
earlier native product crashes are not resolved by this scoped pass. No retry
or relaxed assertion is presented as a fix. Fresh final-head CI, full #88/3D
acceptance and photographed tracking quality remain pending. Android is retired
and excluded. No issue closure is implied.

After #584 merged, this branch was aligned to main `b4133761`. The
[source comparison](data/tracking88-lens-main-base-v1.json) confirms every
product/test/Cargo byte remains identical to validated head `22c3da3d`.
The three add/add conflicts retain the already validated Surface registration
and documentation supersets over the squashed parent versions. No tests were
repeated for unchanged sources. Retargeting the PR to main requires fresh
checks for its final head; the 281-pass record retains its original scope.
