# Tracking #88: nonlinear camera composition routing

This increment follows `78d0e68bed37f8ac7df7bb2a29dee3c8f8fa72a9` and the certified lens primitive.
Nonzero camera distortion now has an actual native composition output route,
with matching spatial projection, picking, drag and sampled selection outlines.
This is a bounded Windows acceptance checkpoint, not completion of #88 or the
3D route. Android is retired. No panel, widget, style, dependency or third-party
asset changed; test fixtures were independently authored.

## Implemented and verified

- **CAM-LENS-1 / runtime ownership:** CameraSample retains a separate LensWarp
  alongside the ideal pinhole matrix. Named/shared frames preserve that value;
  observer cameras replace only the root lens, nested compositions keep their
  own lens, and fractional exposure receivers retain their own calibration.
  Authored/imported model cameras carry no tracking lens. The matrix-only
  calibrated_projection API continues to refuse nonzero coefficients. Invalid
  or uncertifiable calibration is an error even under AuthoredOnly fallback.
- **CAM-LENS-2 / actual spatial composition:** eligible contiguous runs of OBJ
  meshes and planes resolve native depth, MSAA and premultiplied alpha into a
  certified physical overscan raster. The inverse output pass runs before the
  ordinary full-composition overlay. Authored flat-layer boundaries remain flat;
  nested raster output is not distorted again by its parent's camera. Overscan
  is charged to source and shared native allocation limits. Public encoder and
  device ownership guards remain intact; parameter blocks are immutable per
  encode and retained through submission.
- **CAM-LENS-3 / interaction geometry:** project_point now returns a checked
  Result rather than fabricated nonfinite/out-of-domain coordinates. Existing
  valid-point test callers unwrap that result without changing assertions.
  Spatial hit rays invert the same lens; flat and already-rasterized overlays
  retain output coordinates. View-plane drags preserve depth while applying an
  output delta through the forward/inverse mapping. Plane selection edges use
  bounded 16-point sampling per ideal edge. Uncertified anchors/outlines are
  omitted through the existing optional APIs; the checked point API retains
  the cause. This does not add widgets or alter hit tolerances.
- **CAM-LENS-4 / independent pixels:** 4 crop rotations 脳 zero/negative/positive
  radial controls with tangential distortion, real independently authored OBJ,
  transparent planes, depth overlap and a flat separator. Each full-frame
  comparison checks over 250,000 stable pixels using an independent analytic
  Newton ray oracle, with the original fixed two-pixel exclusion and two-byte
  threshold. Actual maxima: 1. Projection,
  picking and drag have independent ray checks; queued complete compositions
  retain distinct lens parameters. Native alpha survives all four precisions.
  Existing primitive controls continue to check HDR and exact identity bytes.
- **CAM-LENS-5 / explicit boundaries:** packed plans and host portable geometry
  output refuse root or nested nonlinear output before serialization. Unsupported
  spatial inputs, including model blending, report composition/frame/object and
  phase rather than rendering an undistorted fallback. Aperture plus lens is
  explicitly refused pending a calibrated gather path. Budget, blend and
  aperture failures have successful real-pixel recovery controls. Retained spatial
  images and actual ExposureFrame tasks explicitly refuse nonlinear temporal
  composition until that path exists; normal misses and flat receivers work.
  Already-retained ideal rasters cannot bypass the guard after camera replacement.
- **CAM-LENS-6 / native executable:** current-source FFmpeg desktop build, MCP
  import of original raw tracking bytes against the repository's original PTS
  fixture, nonzero radial/tangential output, keyed manual camera correction,
  undo/redo, save and read-only reopen. Three captures have independent Newton
  ray pixel checks with zero stable-pixel mismatches. Invalid binding, timing-gap
  and uncertifiable lens controls produce default-level Error records in both
  file and stderr. Info starts are present with MOTION_LOG unset; stdout remains
  strictly parsed JSON-RPC. This synthetic solve on a real video container is
  not a real-footage calibration or scene match.

## Separately explained functional correction

**CAM-LENS-FIX:** the first mesh pixel oracle failed at zero distortion too:
right-side coverage ended at x=560 instead of the independently expected region.
Replacing the OBJ with a plane restored coverage. The fast composition path
was applying the spatial camera to a full-composition model raster that already
contained camera projection. It now uses the same overlay projection as the
compositor and effect path. The original failure PNGs, geometry dump, isolated
model/plane controls and unchanged-threshold passing images are archived. This
is a functional projection fix, not an observability-only disposition. Temporary
test compile errors are retained as history; no failed assertion was weakened.

## Source-bound local acceptance

One serial controller hashed all Rust/WGSL/manifests before and after every
step. No source changed while it ran. Counts from its terminal logs:

| Gate | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| model | 242 | 0 | 0 |
| core | 485 | 0 | 1 |
| render | 168 | 0 | 0 |
| host-tracking | 50 | 0 | 0 |

The core ignored test is the pre-existing manual qualification gate; no new
test is ignored in this checkpoint. All selected renderer GPU tests ran.
FFmpeg desktop check, provenance validation and native executable acceptance
also passed. This does not replace full workspace/desktop or cross-platform CI.
Older compiler access violations and native failures remain unresolved; passing
changed source does not establish that those faults were repaired.

Archive: [source, logs, failures and actual captures](data/tracking88-lens-camera-routing-v1.zip)
(1726844 bytes, 306 members), SHA-256
`912b823daa4b3d6be9819c99463af079f1fe325c0040baa9de23ece42397dfbd`. The JSON closeout records exact source hashes, counts,
pixel metrics and remaining scope. All owned validators were terminal before
archive creation. No cache, artifact or foreign worktree was deleted.

## Remaining scope

- calibrated permitted-source real footage with independent measured residuals; the source-video controls use synthetic camera observations
- vendor adapters, lens replacement/relink/cache invalidation
- actual fractional camera shutter accumulation pixels beyond receiver ownership; nondefault spatial sampling requires an explicit path
- lens plus aperture, spatial depth/masks/mattes/effects/custom blending/nested-3D boundaries explicitly refused until separately implemented
- full maintained workspace/desktop regression and cross-platform CI
- previous native/compiler crash causes and #82 strict P95/GPU coverage failures
