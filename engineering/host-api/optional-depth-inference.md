# Optional desktop depth inference prototype

Build `motion-host` with `--features ai-depth`. The stable model/render closure
does not acquire a Python, ONNX or inference dependency. No inference runtime or
model is bundled, no network is used at inference time, and no Android support
or desktop UI integration is implied by this host API.

## External model preparation

Use a separate directory for the runtime, reference checkout, weights and model.
The validated Python 3.11 environment uses NumPy 2.4.6,
opencv-python-headless 5.0.0.93 and ONNX Runtime 1.31.0. Export additionally uses
PyTorch 2.14.1 and ONNX 1.23.2. These are externally installed packages with their
own licenses; see [attribution](../../ATTRIBUTION.md).

Obtain the official Depth Anything V2 source at revision
`a561b849ebae10a6f5ef49e26c83cbbcd36c71bf` and its Apache-2.0 **Small** weights.
The export tool verifies both the checkout and the exact weight size/SHA-256
before importing the upstream implementation. Base/Large/Giant are not inputs.
With absolute paths, run:

```text
<runtime-python> -B tools/export_ai_depth.py --reference <checkout> --weights <small-pth> --output <new-model-directory>
```

The tool performs no download or package installation. It refuses an existing
output directory, exports a fixed `1x3x518x518` float32, single-file opset-17
model, and checks an actual CPU ONNX prediction against the official PyTorch
forward. It then publishes the model, upstream license and `model.json` together.
The manifest records both the original weight hash and the exported ONNX hash,
tool versions, processing identity and numerical bounds. Different export
environments must retain their own manifest and pass the comparison.

## Host ownership and installation

Construct `depth::onnx::Onnx` with absolute Python, ONNX model and writable
scratch paths and an explicit preferred provider. Construction performs no
model/runtime I/O. Pass it to `depth::Jobs::start`, then submit an `Input`.

An input contains owned opaque **RGB8 in sRGB encoding**, after the declared
crop, clockwise quarter-turn and horizontal mirror. Alpha compositing and
conversion from the renderer's working space belong to the caller. Dimensions
must match the oriented crop and are capped at 4096 per axis. The worker uses
RGB bicubic letterboxing to 518 square, explicit normalization and NCHW layout,
then inverse-crops and resamples scalar depth with aligned endpoints. The
processing identity is `dav2-small-rgb8-letterbox518-align-corners-relative-v1`.

`Key` carries asset ID, source revision, exact rational PTS, full geometry and
`DepthModel`. The model's `sha256` is the **exported ONNX file hash** from
`model.json`, not the original `.pth` hash. The model's revision and processing
identity must also remain explicit. The child verifies the hash of a bounded,
owned model-byte snapshot and loads those same bytes, so replacing the source
file after verification cannot change the model executed by the request.

Poll `take_ready(&live_key)` without blocking the render thread. Obtain the live
key independently from the current source; never reuse the submitted key after
a seek or asset/model/geometry change. Cancel on changes even if no replacement
is submitted. One in-flight job and one replaceable pending input bound retained
work; canceled and stale results cannot install. `in_flight`, `queue_depth` and
lifetime saturating counts expose activity. A backend panic stops that backend.
This lower-level API does not automatically attach a result to a layer. The
optional Session path below owns its installation guard.

## Session-managed static images

With `motion-host/ai-depth`, an owning-thread Session can opt in once using
`configure_depth(backend)`. Passing an `Onnx` uses the same external runtime and
hash-identified model described above. A normal Session creates neither a depth
worker nor an inference runtime. Reconfiguration is rejected; canceling does not
restart a failed backend.

`request_image_depth(asset_id, geometry, model)` resolves the registered image
metadata, then enqueues its managed path. File opening, bounded reading, image
decoding, crop, clockwise rotation, horizontal mirror and inference run on the
depth worker after its Info start and in-flight publication. PNG/JPEG inputs
must be opaque sRGB8 in the declared crop. ICC-tagged, high-bit-depth and
transparent inputs require explicit conversion/compositing and are refused.
No implicit EXIF rotation is applied: geometry describes raw source pixels.
Untagged sources follow the managed-image sRGB convention; arbitrary input color
management is not established by this path.

The encoded file is capped at 64 MiB and source dimensions at 4096 per axis.
Decoded staging checks `8 * source_pixels + 3 * crop_pixels` against the lower
of the Session decoded-image policy and 128 MiB, before decoding. This is a
conservative pixel-payload allowance, not a process-wide codec/RSS measurement.
Static source PTS is exactly `0/1`; composition frame is checked separately.
The source revision is the conservative edit-engine revision. Managed-source
relinking must go through the project edit API; external same-path file writes
are not watched or represented as new project revisions.

`poll_depth()` derives the live identity from Session itself, without taking a
caller-supplied key. Document generation, root, active composition, edit revision,
frame, registered asset metadata and decoded budget must still match. Sampling
also invalidates old requests. Seek, relink, undo, project replacement and budget
changes therefore discard pending/installed results. A new request carries its
own ticket even when the static-image key is identical. `cancel_depth()` clears
ownership; `depth_status()` exposes the bounded worker's flight, pending count,
request, installation flag and saturating lifetime counters.

The installed immutable `Estimate` retains provider, timings and the original
DepthResource semantics. It is not silently attached to a renderer depth buffer.

## Session-managed original video

`request_video_depth(object, geometry, model)` resolves an original video layer
and derives its requested source coordinate from the current composition frame,
clip interval and source offset. Outside the clip/source interval is a normal
`Ok(None)`: no inference is submitted and no error is recorded. This path does
not sample frame blending, effects or composition output.

The depth worker reads the managed source and PTS index, then creates, uses and
drops the native decoder on that same worker. The factory can return a non-Send
decoder. Cancellation is checked between packets; a blocked decoder factory is
observable before entry but cannot be interrupted until it returns. There is no
added thread timeout or restart.

The actual decoded frame must agree with the managed index on PTS, sample end,
dimensions, coded orientation, YUV matrix and range. Geometry uses **display
pixel coordinates** after the decoder's coded rotation; crop, the requested
clockwise quarter-turn and horizontal mirror are then applied. The source PTS
is the actual decoded sample in managed microseconds, not the requested
coordinate or the raw container time base. A seek to another requested coordinate
inside the same VFR sample still invalidates the earlier request ticket.

Native RGB encoding must be explicitly declared for the returned frame. The
initial supported contract requires BT.709 primaries and either IEC sRGB or
BT.709 transfer metadata. sRGB bytes pass through. BT.709 bytes are converted
using the inverse BT.709 OETF followed by IEC sRGB encoding, with one final byte
rounding. This is an explicit signal conversion; it does not apply a display
viewing OOTF or infer mastering intent. A different display-referred conversion
requires a separate policy. Unknown primaries/transfer, HDR and wide-gamut input
are refused before inference; matrix/range alone do not establish the encoding.
The decoder's existing preview pixels are unchanged.

Video staging admits the bounded PTS index and decoded pixel payloads under the
lesser of the Session decoded-image policy and 128 MiB. Source axes are capped
at 4096 and the existing asset contract caps the index at one million timestamps.
These are payload limits, not total codec/process RSS measurements. Polling
independently rechecks document/root, revision, active composition/frame, video
layer, registered metadata, requested source coordinate and budget. Relink and
document replacement therefore reject stale results without a caller-supplied
live key. External same-path writes are not watched; use the project edit API.

These Session APIs do not provide a desktop control, automatic layer effect,
metric calibration or a shared hardware depth binding.

The output is an owned R32Float **unitless relative** resource, with larger
values nearer. Confidence/validity planes remain absent when the model does not
provide them. It is neither calibrated camera Z nor hardware depth; a consumer
must define calibration and projection before using it for metric occlusion.

## Runtime costs, failures and limits

CPU is the functional fallback when the preferred CUDA, DirectML or CoreML
provider is unavailable. The runtime profile identifies providers that actually
executed nodes. Report that result, rather than treating the requested provider
as proof of GPU execution. Internal provider fallback diagnostics remain visible;
the host does not retry failed inference. Only CPU and an unavailable-CUDA to CPU
fallback have been accepted locally; accelerated providers need their own run.

Cost fields separately measure source preparation, prepared RGB bytes,
parent-side staging/serialization/output ownership, load, preprocessing, host
tensor input/output copies, inference and postprocessing. For managed image/video
jobs, total worker wall time includes source preparation. Direct backend calls
have no source preparation; their total includes child startup and temporary
cleanup. The phase fields do not claim to exhaust every orchestration cost.
Provider-internal transfers are included in
inference time. There is no I/O Binding, zero-copy or persistent session claim:
each cancellable child owns its model/runtime and is killed and reaped on cancel.
This bounds ownership but adds Python/model startup cost per request. It is an
optional correctness prototype, not a real-time playback implementation.

The job layer publishes in-flight state and Info start before source IO, decoder
opening or backend entry. Actual video PTS remains unknown until the decoded
sample is validated; source-ready then records it before inference. Errors
retain request/source identity, requested source coordinate, available actual
PTS, phases, elapsed time and bounded original causes. Use the desktop's existing
logger so Error reaches file and stderr and
stdout stays available for CLI/MCP. Normal pending polls and cancellation are
quiet. Child stdout is a bounded private protocol; native diagnostics go to a
separate bounded stderr tail. Temporary files use the explicit scratch root.

Model-reference agreement verifies this host procedure, not physical scene
accuracy. Portraits, illustrations, unusual interiors, transparent/reflective
surfaces and temporal consistency need domain-specific evaluation. Independent
frame estimates have no temporal model and may flicker. Current acceptance and
remaining domain checks are recorded in
[the issue ledger](../validation/ai-depth-current.md).
