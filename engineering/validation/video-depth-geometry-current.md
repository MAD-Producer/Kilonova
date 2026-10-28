# Exact-time calibrated video geometry — issue #87

This increment connects an already admitted original video depth sample to native
composition geometry. It does not complete issue #87 or a calibrated video timeline.
The base is the static image ownership/playback batch at `d4f202124bde90e0b119395b6df71067405cfe9f`.

## Product behavior

- The depth worker retains full display RGBA from the same decoded sample used for
  inference. Indexed PTS/end, source geometry, registered color interpretation,
  original-input fingerprint, cancellation and live document guards remain required.
- BT.709 to sRGB conversion now applies to the retained display pixels before the
  inference crop/rotation/mirror. Both color and inference therefore use the same
  conversion. Original alpha is preserved; the inference crop still requires opacity.
- Scalar planes and retained RGBA capacity share the existing bounded session
  result admission. Codec handles stay on their owning inference worker thread.
- A ready calibrated video layer uses its owned color as an object-scoped GPU
  texture. It enters the existing native shared depth pass with meshes/planes.
  Ordinary video layers still use the existing decoder and BSP paths.
- Source/capability enumeration still reports all video layers. A separate decoder
  inventory excludes only ready owned calibrated video; no second color decode is
  needed for these layers. Pending depth is rejected before output publication.
- GPU color revisions referenced by pending command buffers are immutable. Quality
  and fingerprint are both checked before reuse; unchanged preparation is quiet.
  Returning to ordinary video clears the owned state and requires the proper upload.

## Verification

The pre-change native host regression ran one test and failed with the original
`video geometry routing is not implemented` error. This is a reproduced product
boundary, not a compilation failure or a zero-test invocation.

The accepted source-bound gate runs at the default log level and records raw
commands, stdout/stderr and source SHA-256 before/after every step:

- Host: 109 library tests, 17 image/session depth tests and 15 video depth tests
- Native depth geometry: 8 tests
- Existing native video allocation/instance/lifetime/YUV paths: 20 tests
- Full FFmpeg/AI media library: 56 tests, including its manual timing tests
- Host without inference, FFmpeg/AI desktop compilation, style and provenance checks

All listed tests passed with zero failed or ignored tests. These are affected-path
checks, not a final full-workspace or remote cross-platform acceptance claim.

New native host coverage uses a controlled indexed decoder and real GPU output:
four working precisions, physical near/far occlusion, same-worker color ownership,
source deletion with a fresh headless renderer, no extra decoder opens, retained
byte accounting, and rejection/release after a distinct source-time seek. A second
native test verifies retained BT.709 colors exactly match inference RGB.

The renderer regression checks an independent ray/plane oracle, distinct red/green
source revisions, reversed command-buffer submission, full/preview/full quality,
no repeat upload, same-PTS ordinary decoder transitions and recovery after cache
clear without any source video file. Retained PNGs are native captures; they are
not UI screenshots. Existing assertions were not modified for this increment.

## Evidence archive

- Archive: `data/video-depth-geometry-native-v1.zip`
- SHA-256: `c2afdee395645a5fa147bb9a1643c08d244ed6daecc5cd369aa47dee3b181aed`
- Entries: 60; all archive entries read back and compared byte-for-byte
- Raw gate: `artifacts/pr-queue/video-depth-geometry-green-v1/accepted.json`
- Raw reproduced boundary: `artifacts/pr-queue/video-depth-geometry-red-v1/reproduced.json`

## Remaining requirements

- Video calibration is still saved for one exact prepared RGB/decoded PTS. Moving
  to another composition frame invalidates the admitted live video result; this
  increment deliberately does not reuse static-image time rules for video.
- Bounded per-source-time calibration/admission strategy, continuous video playback,
  real-codec calibrated video acceptance, frozen video-depth plans and complete
  calibrated video movie export remain open. The full media regression exercises
  existing image-depth/frozen outputs; it does not establish these new video goals.
- Complex effects, masks, mattes, nesting/exposure, intersecting transparency, DOF,
  normative scene comparisons and final latest-main desktop regression remain open.
- No Android, panel, style, UI interaction, retry or worker restart changes.
