# Tracking #88: calibrated independent camera runtime

This increment follows `e0a4f958` on `codex/88-track-import-contract`.
It does not close #88, #82, or the entire 3D route. Android remains retired.
No panel, UI presentation, interaction, third-party dependency or external asset
was changed. Geometric fixtures are independently authored. The checked-in
original video retains its existing provenance.

## Implemented boundary and dispositions

- **CAM-1 / new runtime:** project schema **35 only** adds an optional independent
  camera `tracking` binding. The complete required `set_tracking_camera` payload
  selects attachment/track, exact original PTS and camera-local frame anchors,
  signed rational speed, source-world basis, target units/origin, correction
  reference, crop/rotation, near/far, interpolation, gap/confidence and explicit
  missing policy. A missing binding payload is rejected; explicit null removes
  it. The normal core transaction provides locking, atomicity and undo/redo.
- **CAM-2 / original timing:** immutable admitted observations remain separate
  from ordinary camera tracks. Runtime uses the original signed source PTS,
  fractional local frames and rational speed. Translation, shortest quaternion
  slerp and focal/principal values interpolate between valid source samples.
  Reverse, freeze, clip-local time, signed quaternion equivalence, gap and
  confidence policies have direct model tests. No integer key baking,
  extrapolation, source repair or nominal-FPS substitution is introduced.
- **CAM-3 / projection:** explicit source RDF camera-local axes and selected
  world basis/units convert camera-to-world pose. Off-centre principal point and
  independent fx/fy produce a calibrated pinhole projection. Explicit plate
  crop, resize and all four rotations affect the entire projection, including
  off-diagonal coefficients. Independent numeric controls verify reprojection
  and wgpu 0..1 near/far depth. This is a functional camera consumer, not only
  import observability or a source-FOV approximation.
- **CAM-4 / corrections:** runtime composes authored spatial parent once, then
  `A(frame) * inverse(A_reference)` and the raw converted pose. Authored ordinary
  position/target/roll/orbit tracks remain independent manual corrections.
  The authored/reference tangent ratio supplies the focal multiplier. The
  original ordinary view calculation now also returns its inverse directly;
  recovering it from projection-times-view introduced real numerical error.
  Authored camera and imported source camera call paths retain their explicit
  resolver requirements. No source camera silently uses an ordinary substitute.
- **CAM-5 / normal inactive branch:** a strict tracking solve can legitimately
  be unavailable after a shot cut. Scene retains that inactive camera's exact
  typed cause, bounded by the existing project node count, instead of failing
  an unrelated active shot or publishing a fabricated identity pose. Explicit
  view selection returns the retained cause. Active cameras and cameras in a
  spatial object's ancestry remain strict. Successful inactive cameras stay
  available to the existing shared-frame fork path. Every new sample resets
  the bounded causes, and returning to a valid source frame recovers. This
  changes unused tracking-source selection behavior; it is not a generic
  catch-and-continue for ordinary/imported cameras or other spatial objects.
- **CAM-6 / new error coverage:** binding/sample errors identify object,
  attachment, bounded track, local frame and original availability/geometry
  cause. Existing worker observations supply request, operation and elapsed
  context after guards release. Default Info start precedes source decode and
  render work; Error reaches both file and stderr; stdout stays JSON-RPC only.
  Healthy sampling adds no logging or filesystem IO. Inactive unavailable
  sources are normal until explicitly requested, not per-frame warnings.
- **CAM-7 / functional boundary:** nonzero radial or tangential distortion
  requires a real nonlinear output stage. The current pinhole consumer refuses
  it with actual coefficients even under `authored_only`; silently discarding
  lens calibration would be incorrect. Missing policy only covers unavailable,
  outside-range, lost/low-confidence or excessive-gap observations. Nonzero
  distortion output and calibrated real-footage qualification remain pending.

## Executed native evidence

The source-bound native controller built and ran actual FFmpeg-enabled
`motion-studio`. It admitted all 72 native PTS of the checked-in VFR/B-frame
original through `import_tracking`. The independently authored solve includes
camera translation, yaw, changing fx/fy and off-centre principal point. Three
primary-colour 3D cards at different depths provide independent fiducials.
No imported model, downloaded asset or mocked successful renderer is involved.

An independent Python RDF pinhole calculation supplies projected anchors and
all card corners without using engine camera matrices. Anchor residuals are
below 0.003 pixels. Three actual GPU captures cover frame 0, moving frame 30 and
frame 30 with independently keyed camera position and target corrections.
All three captures were visually inspected. Every card in every image has zero
silhouette mismatches outside the fixed two-pixel edge exclusion declared before
execution. This exclusion does not qualify antialiasing or subpixel coverage.

| Output | Card | Interior pixels | Exterior pixels | Mismatches |
| --- | ---: | ---: | ---: | ---: |
| Reference | 700 | 238 | 298066 | 0 |
| Reference | 701 | 154 | 298150 | 0 |
| Reference | 702 | 108 | 298196 | 0 |
| Yaw and translation | 700 | 288 | 298016 | 0 |
| Yaw and translation | 701 | 180 | 298124 | 0 |
| Yaw and translation | 702 | 130 | 298174 | 0 |
| Manual correction | 700 | 270 | 298034 | 0 |
| Manual correction | 701 | 180 | 298124 | 0 |
| Manual correction | 702 | 130 | 298174 | 0 |

The original external solve was removed before undo/redo and manual edits.
Managed bytes retain exact whitespace and source manual-correction flags. Two
ordinary position keys and two target keys persist independently of unchanged
raw observations. Save/readonly reopen reproduces the corrected project. An
absent-track edit is atomic. A zero-gap fractional seek fails; restoring the
policy recovers. A genuinely nonzero source radial coefficient is refused;
seeking a supported observation recovers. Both default-level file and Error
stderr contain binding, gap and lens causes with request/elapsed context. Every
stdout line is a matching JSON-RPC response. External Epic Vulkan overlay-loader
warnings were not suppressed or treated as application defects.

## Failures, regression and remaining limits

The first local numerical tests failed with approximately 0.004 pixels of
projection error and an incorrect manual translation delta. Production recovered
the ordinary view from projection-times-view. Returning the inverse view directly
fixed that cause; the original precision assertions were kept unchanged. Failed
logs are retained alongside passing logs. No retries, timeouts, restarts, new
ignores, weakened existing assertions or substituted successful observations
were introduced to make acceptance pass.

Exact-source local regression: full model **238 passed / 0 failed / 0 ignored**,
full core **485 passed / 0 failed / 1 existing manual benchmark ignored**,
and selected host
tracking **50 passed / 0 failed / 0 ignored**. The host filter contains 49 tracking
unit controls and one existing resource-tracking integration control. FFmpeg
desktop check and provenance guard both passed. Full model/core include doc tests.
No new ignore or weakened existing assertion was added. These selected host
controls do not constitute a full-host rerun or native-crash repair.

The expanded renderer gate **failed before tests ran**, while compiling the
default/gpu-tests `motion-render` library: rustc 1.97.1 exited with
`0xc0000005 / STATUS_ACCESS_VIOLATION`. Windows events identify
`rustc_driver-e41b336de1bfe42c.dll`, RVA `0x2df4dec`. Exact compiler DLL/PDB,
events and WER report were retained privately; the temporary dump was no longer
available. A read-only nearest-symbol lookup is not an unwound call stack or a
proven cause. Neither shared/explicit/multiple-view tests nor the subsequent
imported-camera test target ran. This failed target was not retried unchanged.
Remaining host/check/provenance gates ran separately; their renderer dependency
disables default features, so they do not retry or replace that failed target.
The first remaining-controller generator selected `steps=[]` inside the report
instead of the step declaration and produced a SyntaxError before execution or
directory creation. Only the generator was corrected; the failed script is kept.
This checkpoint is **not qualified for merge or issue closure** by these logs.


Full maintained desktop/workspace and cross-platform CI are not qualified by
these selected controls. Earlier full-host Naga/native faults and MCP test
compiler access violation remain unresolved historical failures. #82's original
strict P95 failures and incomplete GPU coverage remain failures. No unchanged
failing gate was rerun simply to obtain green.

Remaining #88 scope includes nonlinear lens output, calibrated real footage,
vendor solve adapters, relink/cache/exposure integration and full exact-head
qualification. This synthetic known-camera control verifies the new calibrated
consumer; it is not automatic camera solving or proof of a real-footage match.

Archive: `tracking88-camera-binding-v1.zip`, 819301 bytes, 64 members; SHA-256 `afe915e01847767262f050fff466684fa7c287368649fcafe5bd790af01130c8`. Every archived member was read back and byte-compared. Exact source hashes are in `data/tracking88-camera-binding-closeout.json`.
