# Depth #74: actual inference to frozen 3D output

This checkpoint runs the real Session, native FFmpeg decoder, installed CPU
ONNX model, public indexed-video calibration command and frozen PNG exporter
on source `dca6e57f3572d967bf6e5edf2362b500146b9ed9`, rebased onto main
`ead8ce0c`. It adds external acceptance evidence; no product code, panel,
existing Cargo test assertion, backend or stack setting changes.

## Executed path

The independently authored H.264 fixture retains its original VFR timestamps,
nonzero start time and source identity. At composition frame 2 the requested
source time is 76,667 microseconds; decoding selects indexed PTS 50,000.
The 96-by-64 source is cropped to `[7,5,80,52]`, rotated one clockwise quarter
turn and mirrored horizontally. The actual inference input is 52 by 80 RGB.
Its bytes exactly match the previously verified prepared-source fixture.

The installed, previously attributed Small model has SHA-256
`274368034c0fcef08f9b197d33b606bae9c578acca861ef72feb268143b21434`.
Both calibration requests execute actual CPU inference. The second result is
bit-identical to the first, after committing the calibration invalidates the
old live binding and requires a fresh request. The retained historical,
independently generated PyTorch golden has normalized maximum error
`7.787024532934269e-7` and mean error `1.2020595162418164e-7`, below the original
`2e-4` and `2e-5` limits. This is comparison with a pinned reference, not a new
PyTorch execution.

Relative predictions are explicitly authored into 8-metre and 4-metre anchors
at the observed minimum and maximum, using camera-Z interpolation and 20 scene
pixels per metre. These distances are scene choices, not measured physical
accuracy. The source photograph's saved camera and world matrices determine
the projection. A separate opaque blue plane intersects the depth range.

The public `video_depth_calibration_command` records the indexed source-time
interval. After fresh binding, two `FrameFreezePlan`s capture the calibrated
scene and a flat control made with the public calibration-clear command.
All live Estimate owners and the Session are dropped before both plans export
actual PNG files. The original source file remains available. This verifies
frozen ownership across Session closure, not export after source deletion.

## Independent image qualification

The external NumPy oracle uses f64 ray/triangle intersections and the saved
authoring camera. It reconstructs inference-grid positions from the declared
source geometry and scalar field; it never reads production mesh vertices or
calls production renderer helpers. The analytic plane, tilted plane and sRGB
roundtrip self-checks also pass.

The unchanged qualification requires nine within-pixel rays to agree on the
occlusion class, more than two scene units of separation from the blue plane,
an interior crop margin of 0.03 and no more than two bytes of within-pixel
color variation. Each accepted pixel must have opaque alpha and RGB error at
most three bytes; both near and far classes must contain at least 30 pixels.
The flat control must be blue within one byte on the same checked pixels.

| Actual output | Far checked pixels | Near checked pixels | Maximum RGB error |
| --- | ---: | ---: | ---: |
| 384 by 256 frozen PNG | 27,884 | 330 | 1 byte |

All 28,214 checked pixels pass, including the flat control. The remaining
70,090 pixels are explicitly excluded by the declared qualification. Both
complete PNGs were visually inspected. This is an SDR 8-bit PNG checkpoint,
not full-image equality, HDR export, an antialiased-edge qualification, a GUI
capture or a test of every working precision.

The successful run takes 117.313 seconds including the independent CPU oracle.
Its native executable SHA-256 is
`3a412c04bd27b08bd846ac070061809e2c1f5f6ecbc5a0126019c9b8e902f951`.
The maintained product source closure and private probe hashes remain unchanged
throughout that run.

## Failure paths and retained attempts

Before inference, deliberately removing the newly created video index produces
the original `video cache missing; call prepare_video` failure. At default
Info, the file contains the start record before the failure; the Error reaches
both the file and stderr. Product stdout is empty. Restoring the index permits
the next real request to succeed. Existing Vulkan loader errors about missing
Epic overlay manifests remain visible, without any layer or registry override.

The archive retains the unsuccessful harness attempts separately:

- v1 calls the static-image calibration command on video and is correctly
  refused during frozen capture. The product already provides the indexed-video
  interface; this is a harness error, not a repaired product defect
- v2 fixes the interface and exports both 96-by-64 PNGs. Its first oracle stops
  on an omitted optional JSON field. Correcting only that read then checks
  1,685 far pixels and zero stable near pixels, failing the original minimum
- v3 increases the validation viewport to 384 by 256, retaining the same source,
  model, crop, authored geometry, calibration policy and numerical thresholds.
  The finer sampling supplies qualified near pixels; no original failed record
  or assertion is removed

## Complete regression remains pending

The complete maintained-workspace gate on this rebased source is **not passed**:

```text
cargo test --workspace --exclude motion-android --features motion-desktop/ffmpeg,motion-host/ai-depth,motion-desktop-media/ai-depth --no-fail-fast --locked -- --test-threads=1
```

The first attempt stops before compilation because the offline cache lacks a
main-required package. The next attempt retains `--locked`, downloads the
required packages and compiles for 1,286.688 seconds before `link.exe` reports
`LNK1180: insufficient disk space to complete link` for
`motion-render/tests/generator_error_boundaries`. Cargo exits 101; **zero test
invocations and zero test summaries execute**. Source hashes remain unchanged.
The original logs are retained. No assertions, ignores or dependencies change
to bypass the gate, and this narrow inference/render pass does not replace it.

Earlier native full-regression access violations remain unresolved; this
checkpoint does not demonstrate their repair. Final-source cross-platform CI,
full maintained regression and broader issue acceptance remain separate.
Android is retired and excluded by the user's maintenance scope.

## Evidence

[Source closures, all three attempts, actual PNGs, original workspace failures,
commands, numerical reference and independent oracle](data/depth74-inferred-render-dca6e57f.zip):
1,048,164 bytes, 92 members, SHA-256
`b4a649c531f9ec9c8a1159e15d944dc58bd03cc20f84a24ddc4ae22dbae7931e`.
Every member was read back and verified; the adjacent JSON contains its manifest.
Installed plugin/cache/scratch directories are excluded. No binary, inference
runtime, model weights, new third-party asset or Adobe content is added.
