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
This native static-image Session API does not implement video decoder-PTS
staging, a desktop control, automatic layer effects, or metric calibration.

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

Cost fields separately measure parent-side staging/serialization/output ownership,
load, preprocessing, host tensor input/output copies, inference and postprocessing.
Total backend wall time also includes child startup and temporary cleanup; the
phase fields do not claim to exhaust every orchestration cost.
Provider-internal transfers are included in
inference time. There is no I/O Binding, zero-copy or persistent session claim:
each cancellable child owns its model/runtime and is killed and reaped on cancel.
This bounds ownership but adds Python/model startup cost per request. It is an
optional correctness prototype, not a real-time playback implementation.

The job layer publishes in-flight state and Info start before entering the
backend. Errors retain request/source identity, phases and bounded original
causes. Use the desktop's existing logger so Error reaches file and stderr and
stdout stays available for CLI/MCP. Normal pending polls and cancellation are
quiet. Child stdout is a bounded private protocol; native diagnostics go to a
separate bounded stderr tail. Temporary files use the explicit scratch root.

Model-reference agreement verifies this host procedure, not physical scene
accuracy. Portraits, illustrations, unusual interiors, transparent/reflective
surfaces and temporal consistency need domain-specific evaluation. Independent
frame estimates have no temporal model and may flicker. Current acceptance and
remaining domain checks are recorded in
[the issue ledger](../validation/ai-depth-current.md).
