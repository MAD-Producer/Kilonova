# Native prerender producer checkpoint (#82)

Status: implementation authored; compilation and actual controls are pending.
This document does not establish the complete background-prerender capability.
Android is retired and excluded from this desktop checkpoint.

## Implemented boundary

`PrerenderPlan::capture` runs on the Session owner and captures the immutable
document, effect registry, source handles, depth snapshot, platform factory and
the **same live allocation budget**. It excludes audio decoding. Materialization
and GPU initialization belong on the background worker. `PreparedPrerender` can
cross threads; `NativePrerender` deliberately cannot. No Session is transferred.

Content keys hash the actual private source copies, serialized frozen document,
resolved effect-package bytes and frozen depth payload. They do not substitute
a revision, file path, mtime, queue range or export destination for content.
Hashing streams bounded chunks and checks cancellation. Existing snapshot byte
and entry limits apply; traversal also bounds directory depth. A project with
extra nonvisual data may produce a conservative cache miss.

The output profile is explicitly full-resolution, authored camera, linear
premultiplied native RGBA at the project's Unorm8, Unorm16, Float16 or Float32
precision. Preview quality and observer modes are not covered by this profile.
For Unorm8, native color storage is `Rgba8UnormSrgb`: the physical bytes are
sRGB-encoded while sampling produces the declared linear values. Other native
working formats are Rgba16Unorm, Rgba16Float and Rgba32Float. Raw readback retains
the actual storage without an additional transfer or reinterpretation.
Exact fractional frame time and owner document/device generations remain in the
frame key. Required optional GPU features are checked by the existing renderer;
there is no precision fallback.

The producer samples a fresh scene with the owned model catalog, verifies frozen
depth, synchronously prepares original images, waits for exact video frames, then
renders and reads native bytes. Incomplete image preparation is an error and
cannot publish a black completed frame. Authored exposure uses the persistent
exposure task and native exposure readback. The existing refusal of combined
frozen calibrated depth and exposure remains explicit. No window surface,
Present or editor chrome participates. Readback adds no display transfer, clamp
or alpha conversion. The existing renderer's precision policies are retained:
Unorm8 exposure resolve clamps straight RGB and repremultiplies before its
native quantization; floating-point exposure retains HDR and negative channels.

Initialization and frame starts are Info records before GPU/decode work. Phase
callbacks precede the corresponding work, enabling the caller to publish state
and cancel. Actual failures retain request/frame/phase/elapsed context at Error;
normal cancellation is not an Error. Decoder cleanup failures are logged without
replacing a primary render failure. GPU allocation is admitted through the live
shared budget. The explicit CPU frame limit is checked before GPU initialization
and completion. This producer retains no completed-frame collection.

## Controls to execute

- Actual frozen-source key changes and independence from later original deletion,
  queue scope and revision; fractional keys and invalid-frame refusal
- Cancellation and CPU frame admission before initialization/publication
- A real independent rendering thread, original image deletion, animated endpoint
  pixels, unchanged live snapshot/time/history and Session closure
- Cancellation at the actual Draw and Readback phase, no readback publication,
  then successful rendering with a fresh cancellation token
- Lowering the same live GPU budget, actual refusal, restoring it, successful
  native output and GPU allocation retirement
- Explicit manual all-four-precision native HDR/negative/alpha pixel checks,
  both ordinary composition and authored exposure, without SDR conversion

None of these authored controls is counted as passing until the source-bound
controller records successful execution. No existing assertion was changed.
The first source-bound execution retained five successful focused controls and
one failure in the newly authored moving-image control: its expected storage
was incorrectly Rgba8Unorm. The implementation returned Rgba8UnormSrgb, matching
both the existing `color_storage(Unorm8)` mapping and ImageContract negotiation.
Only the new format/byte oracles were corrected to that verified contract;
the original failure remains in `native-prerender-producer-v3`. The all-precision
oracle now expects encoded native blue 99 for a linear sample of 0.125 rather
than linear-UNORM byte 32. No product format or rendering behavior was changed.
The subsequent explicit manual run retained another new-oracle failure: Unorm8
exposure returned red 137 instead of 255. `renderer/exposure.wgsl` independently
defines this policy: clamp straight red to one, multiply by quantized alpha
64/255, then encode native sRGB (137). Ordinary Unorm8 rendering instead saturates
the HDR premultiplied red to 255. The new control now distinguishes these two
existing policies; float controls still require exact HDR, negative and alpha
values. The failed run is retained in `native-prerender-producer-v4`; no shader,
product return value, existing assertion or tolerance was changed.
The procedural source/effect fixture is independently authored in this repository;
no new dependency, third-party asset or Adobe binary/resource is introduced.

## Remaining integration

The owner scheduler still needs to launch/cancel bounded actual jobs, retain at
most its admitted completion quota, reject obsolete tickets and upload/cache
completed native bytes on the foreground renderer. Cross-device RenderTargets
must never be reused. Foreground consumption must use a matching output profile;
full authored output is not interchangeable with a reduced/observer preview.
GPU driver operations cannot be preempted inside a submitted call. The current
phase cancellation contract is not a foreground latency/P95 proof.

Complete #82 acceptance also needs real foreground cache hits, invalidation on
edits/source/device changes, pressure behavior and measured playback/seek P95.
Optional disposable disk storage remains absent. The public background-prerender
capability stays disabled until that complete runtime is implemented and verified.
