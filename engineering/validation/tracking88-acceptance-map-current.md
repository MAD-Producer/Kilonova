# Tracking #88: integrated acceptance map

The latest [photographed frozen-output checkpoint](tracking88-frozen-precision-current.md)
covers all four working precisions and twelve actual SDR PNG exports after
live-layer deletion and Session closure. The underlying photographs remain
separate calibrated poses; continuous motion and complete regression are still
outside that checkpoint.

Issue #88 remains open. This map distinguishes the implemented imported-data
route from optional tracker research, full regression, and the broader 3D route.
Earlier `*-current.md` files are chronological increment reports: a remaining
item in an early report may be implemented by a later linked increment. It is
not evidence that every later acceptance boundary is closed.

## Original issue requirements

| Item | Current evidence | Remaining boundary |
| --- | --- | --- |
| I1: versioned point/plane/external-camera data, source geometry/time, calibration, axes, units, confidence | Asset and bounded JSON contracts, original source/decoded timeline admission, managed raw documents, pose and source-time tests | Vendor-specific adapters and a built-in full 3D camera solver are not delivered by this contract |
| I2: map independent tracks to objects/parent/camera without baking ordinary camera keys | [Point](tracking88-point-binding-current.md), [plane](tracking88-plane-binding-current.md), [camera](tracking88-camera-binding-current.md) and [nonlinear lens routing](tracking88-lens-camera-routing-current.md), with explicit missing/interpolation policies | The explicitly refused lens/aperture, temporal and spatial-feature combinations remain separate limits |
| I3: residual and replacement evidence before prototyping a tracker | [Independent photographed calibration](tracking88-photographed-calibration-current.md), [video identity](tracking88-video-publication-current.md), [paired replacement](tracking88-paired-replacement-current.md) | The separate #582 optional tracker has failed its complete photographed sequence gate; synthetic success does not qualify real tracking |
| I4: saved manual corrections/failure states and traceable original solve | Asset roundtrip, independent corrections, managed original bytes, paired history, reopen, and [MCP workflow](tracking88-mcp-replacement-current.md) | File acquisition verification is not a continuing lease against external workspace mutation |
| A1: known synthetic camera roundtrip and real calibration, measured independently | Held-out OpenCV photographs, native measured-corner residuals and independent GPU ray oracles | Static photographs do not establish continuous photographed camera-motion solving; optional real-video tracker quality remains failed |
| A2: mismatched fps, crop/rotation, distortion and axis conversions | [Exact source clock tests](tracking88-time-mapping-current.md), [pose conversion](tracking88-pose-conversion-current.md), all four rotations and nonzero radial/tangential native lens controls | Source-time arithmetic does not complete the still-open project rational-rate issue #85 |
| A3: aligned output, relink, undo and cache invalidation | Native original-video decode, deliberate wrong-source rejection, atomic picture/audio/solve replacement, GPU capture, history/save/reopen, actual stdio MCP, [camera/video Surface front](tracking88-surface-current.md) and [point/plane/nonlinear video Surface and queued input oracles](tracking88-native-video-lens-current.md) | These bounded Surface cases do not cover every codec/lens/binding, the prototype's real-footage quality, or arbitrary external workspace mutation |

## Regression and delivery

The earlier full FFmpeg desktop binary run failed three native subprocess
wrappers (1522 passed, 3 failed, 50 existing ignored). Windows recorded access
violations, but their root cause was not established. Isolated passing reruns
and the new [child-identity diagnostics](tracking88-native-child-process-current.md)
do not erase that failure or prove its repair.

A complete FFmpeg desktop **package** run on combined source `52ce7ebf` passed:
1542 passed, zero failed, 50 existing ignored. It includes the binary and all
five integration targets and uses `--no-fail-fast`. The original validator
exited with code 0 and all source hashes/target coverage were checked. See the
[complete package record](tracking88-desktop-package-current.md). It precedes
the new Surface test and is not a complete maintained-workspace gate.

PRs #580 and #581 are formal increments. Their earlier heads passed required CI;
the updated parent/main integration heads require fresh checks. #582 is the
separate failed real-footage prototype draft. #583 and #584 contain the bounded
replacement transaction and MCP workflow; their current checks and readiness
must be verified independently. This index does not authorize merging them.

The original prerequisites #85 and #86 remain open. Android is retired and
excluded by the user's maintenance scope. Neither a documented disposition nor
a green subset completes #88, its prerequisites, or the full 3D route.

## Historical layer-clock integration

The [layer-clock integration](tracking88-layer-time-integration-current.md)
combines main `9d085c0f` with the video/lens fix: complete model/core packages
passed 859 tests with one existing manual ignore; seven complete renderer
targets passed 100 tests; two actual Windows Surface controls passed. These
source-bound results supersede the old-main boundary for those selected gates,
not the full workspace, entire desktop binary, photographed tracker or final CI.

This is evidence for the original integration source, not a new execution on
the combined branch after synchronizing the later video prefetch baseline.

## Layer-time baseline synchronization

The root [main integration](tracking88-root-main-integration-current.md), native
[footage clock combinations](tracking88-video-clock-current.md), [paired edit
history](tracking88-pair-clock-integration-current.md), and [MCP/native Surface
workflow](tracking88-mcp-clock-integration-current.md) record the synchronized
source and their newly executed gates. PR #583's frozen source passed 1060 tests
with one existing manual ignore; PR #584 separately re-executed three MCP tests,
one actual Surface test, and ten actual executable stdio controls. Five complete
crate trees are identical to #583, with their Git identities recorded. These
results do not constitute a new full desktop/workspace run or final-head
cross-platform CI pass. The full 3D route and issue #88 remain incomplete.


## Current-main prefetch and nonlinear-video synchronization

The [latest combined-source record](tracking88-lens-clock-current.md) adds freshly
executed complete host-library and seven renderer-target gates, actual Surface,
original-video clock and current-main prefetch controls. It explicitly includes
main e2347558. Historical full model/core results refer to identical crate trees;
no new full desktop/workspace or final-head CI pass is claimed.


## Post-merge current-main validation

The [new source-bound integration](tracking88-lens-current-main-v2.md) covers
parent 52b26fda and main 0790404f after #583 merged. Its 281 passes include
seven complete renderer targets, the complete host library and actual native
Surface, prefetch and original-clock controls. Earlier counts retain their
original scope; this is not full desktop/workspace or final CI acceptance.
