# Issue 88: photographed external camera import and certified lens inversion

The later [four-precision frozen PNG checkpoint](tracking88-frozen-precision-current.md)
executes twelve current-source exports after deleting live layers and closing
their Sessions. It extends the bounded output evidence below; it does not
complete continuous camera motion, reconstruction or full regression.

Three independently calibrated photographed board poses pass actual vendor import,
binding, native pixels, undo/redo, save and fresh read-only reopen. This is an
OpenCV-calibrated COLMAP text-format interchange test, not execution of a COLMAP
multi-view solver or continuous camera-motion acceptance. A board square defines
one source unit; the physical square size and surveyed metric scale are unknown.
Android remains excluded and issue 88 stays open.

## Calibration and functional fix

The same pinned thirteen Apache-2.0 OpenCV photographs are used: ten calibration
views and three held-out poses. The eight-parameter OPENCV profile is independently
re-estimated with k3 fixed to zero during fitting; an existing three-radial fit is
not truncated. For each held-out pose, 27 measured corners fit extrinsics and the
other 27 measure error. OpenCV centre-origin coordinates become COLMAP edge-origin
coordinates by adding 0.5 to principal points and observations. Original image
bytes, detected measurements, options, tool versions and licenses are retained.

The first native fixture supplied null where the current track schema requires
an absent optional key. Correcting that private fixture exposed a real product
failure: the previous unit-step fixed-point certificate rejected the unchanged
photographed lens. After the fix, the next observer incorrectly compared a
projected XYZ anchor to OpenCV XY; its correction selects XY by the same object
ID and additionally checks finite XYZ and normalized depth. All original
assertions, including the 0.0003-pixel threshold, remain. These original failed
observers and the actual product failure/default Error records are retained.

The functional fix certifies q <- q + step*(d-F(q)) on a complete enclosing disk,
using directed interval arithmetic for contraction and self-mapping. Cubic
Bernstein control hulls enclose radial intervals for relaxed steps. Step one
keeps the existing algorithm. CPU and shader share an exactly representable
binary-fraction step; overscan includes the corresponding inverse-Lipschitz
factor. The contraction limit remains 0.45, iteration count 48, enclosing radius
at most twice the distorted-domain radius, and final residual limit unchanged.
Folded or uncertified domains remain errors. An earlier conservative interval
attempt failed and is retained with exact source snapshots.

## Executed scope

| Gate | Passed | Failed | Original ignored | Filtered | Targets |
| --- | ---: | ---: | ---: | ---: | ---: |
| focused-lens | 6 | 0 | 0 | 0 | 2 |
| motion-model | 340 | 0 | 0 | 0 | 55 |
| motion-core | 570 | 0 | 1 | 0 | 89 |
| native-render | 170 | 0 | 0 | 0 | 10 |
| host-lib | 180 | 0 | 0 | 0 | 1 |
| desktop-ui | 1976 | 1 | 72 | 0 | 31 |

The focused six lens tests are included in complete model totals and must not
be added twice. Model controls check 63 independently generated OpenCV inverse
rays, four rotations, unaltered calibration, exact shared step, ordinary lenses
and folded-domain refusal. Actual nonconstant GPU output uses an independent
analytic-Jacobian Newton oracle and checks all sixteen precision/rotation pairs.
Float paths retain negative/HDR values and alpha. Their per-pair errors and
unchanged numerical thresholds are in the receipt.

The first desktop/UI attempt stopped during compilation without a terminal
receipt: on resumption its original tool handle, controller and Cargo process
were absent, stdout was empty and no tests had run. Its unchanged running
receipt and raw output are retained beside an additive observation; the cause
of interruption is unknown. The second attempt uses the same source and command
in a new output directory. It finished all 31 target invocations in 2700.765
seconds with 1976 passed, one failed, 72 original ignores and no filters. The
failure is the native toolbar screenshot parent's assertion that its child
exited successfully; the captured child output names no underlying cause.
A subsequent direct execution of that exact child and unchanged test body
passed in 16.25 seconds, preserving its logs and native captures. This does
not turn the failed full run into a pass or establish its cause. Complete
desktop/UI regression remains an unmet gate. Desktop/UI execution uses the previously
documented test-only codegen-units=4 overrides. No original assertion, time budget, ignore, stack,
adapter, production profile, panel, layout, style, interaction or dependency was
changed for acceptance. The executable uses the normal dev/FFmpeg build without
those test overrides. Earlier failures remain separately recorded; the passing native photographic
workflow and direct toolbar diagnostic do not establish their root causes.

## Actual native photographic comparison

| Held-out photo | RMS px | Maximum px | Stable pixels | Native/oracle max byte error |
| --- | ---: | ---: | ---: | ---: |
| left03.jpg | 0.175040 | 0.331772 | 282516 | 0 |
| left08.jpg | 0.263063 | 0.563161 | 285275 | 0 |
| left14.jpg | 0.215038 | 0.426463 | 286158 | 0 |

Seven fresh processes execute the real stdio MCP protocol. Import preserves the
original text/map/options and authored camera, import and binding have separate
undo/redo checks, bad binding is atomic, and deleting the external original does
not affect managed bytes on fresh reopen. Independently projected anchors must
agree within 0.0003 pixel. Dense independent ray/plane pixels use the original
two-pixel edge exclusion and two-byte RGBA/alpha limits; black and white linear
composites are separately checked. This qualification covers native Unorm8
preview, not all-precision photographed final export.

At default Info the actual file contains non-Present starts. Original bad-track
causes occur in both file and stderr; every stdout line parses as JSON-RPC.
Executable SHA-256: `1e80ff7d026cdbb15da4e9d72468e3f3a691e7af2a6a0cb56f31ebfed5bad010`.

The [archive](data/tracking88-colmap-photographs-v1.zip) and
[receipt](data/tracking88-colmap-photographs-v1.json) contain exact accepted and
failed sources, controllers, raw outputs, source closures, calibrated data,
unchanged photographed originals and licenses, actual projects, managed vendor
bytes, captures and comparison image. Every member was read back and hash-checked.
The frozen base is `e38bcec150614b0a8d337185650ee5ed7fe12d21` plus the archived lens change; this is
not evidence for any later product change. Final exact-head cross-platform CI,
real continuous tracking quality, cache/reconnection completion and issue-level
closure remain separate requirements.
