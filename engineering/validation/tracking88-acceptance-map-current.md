# Tracking #88: integrated acceptance map

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

PRs #580 and #581 are formal increments with passing required CI. #582 is the
separate failed real-footage prototype draft. #583 and #584 contain the bounded
replacement transaction and MCP workflow; their current checks and readiness
must be verified independently. This index does not authorize merging them.

The original prerequisites #85 and #86 remain open. Android is retired and
excluded by the user's maintenance scope. Neither a documented disposition nor
a green subset completes #88, its prerequisites, or the full 3D route.
