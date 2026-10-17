# Desktop native frame layout validation

This increment addresses two desktop items in #131: validating native strides
before creating Rust slices, and initializing RGBA export row padding. It is
stacked on `fix/keyframe-collisions` (PR #310, base commit
`e6e2bf56dd48ec671734a453066077a05c335901`), following PR #309 and PR #307.
The other media, font, progress, and Android items require separate review;
this increment does not close #131.

## Native boundary

`ffmpeg-next` 9.0.0 exposes a video plane with `data`/`data_mut` by casting the
signed native linesize and constructing a slice of `stride * plane_height`
bytes. Checking the downstream YUV packer's geometry cannot protect that earlier
slice construction. Current normal software decoders supply positive strides;
this change guards the native boundary rather than claiming a reproduced crash
with an ordinary input video.

Before any decoded plane is exposed, the desktop backend now checks every
required plane's signed stride, row width, non-null data pointer, checked full
byte span, and containment in its live native backing buffer. Dimensions must be
positive and within the existing video dimension limit. The span includes the
last row's padding, matching the wrapper's actual slice length. Planar YUV420P /
YUVJ420P and interleaved NV12 / NV21 retain their existing packing behavior.

The small unsafe boundary only reads metadata from an owned, live AVFrame and
obtains a borrowed buffer reference with `av_frame_get_plane_buffer`. It creates
no pixel slice. It relies on the native library's valid AVFrame/AVBufferRef
ownership contract; it cannot recover from arbitrary native memory corruption.
Negative strides are explicitly rejected, without attempting a vertical flip.

The RGBA export writer validates its layout and input before writing, clears
the entire exposed plane, and then copies/composites the visible rows. Existing
straight-alpha compositing onto black in linear light is retained. This
initializes row padding on every write, including when reusing a frame.

## Error propagation

Validation failures return descriptive `Err` values with the plane index and
cause. Decode conversion propagates them through `Decoder::frame`; the existing
video worker records them in its error state/diagnostics and returns them to
the host request. Probe failures return through the existing import task.
Export failures propagate through `Mp4Encoder::video_frame` and the existing
movie task error path. No error is converted into a successful frame.

## Verification

The new native tests mutate only metadata on owned allocated test frames,
restore that metadata before observing pixels or dropping the frame, and never
call the wrapper's pixel slice methods while the layout is invalid. They cover
negative, zero, short, and oversized strides; missing planes; null and
out-of-buffer addresses; invalid dimensions; and missing backing buffers.

Conversion tests exercise the actual entry point, compare packed pixels and
RGBA results across planar/interleaved formats, include odd sizes and all four
supported rotations, and check plane-specific error returns. Export tests
prefill padding with a sentinel, verify every exposed padding byte is zero,
check opaque pixels and linear alpha compositing, and verify rejected writes
leave the existing plane unchanged.

Run the complete desktop media suite, including the existing real decoder and
MP4 export round trips:

```text
cargo test --locked --offline -j2 -p motion-desktop-media --features ffmpeg \
  -- --test-threads=1
```

The local environment is Windows with the repository's portable MSVC setup,
FFmpeg 8.1 development libraries and `LIBCLANG_PATH`. The final run uses a
dedicated `CARGO_TARGET_DIR` at
`artifacts/workspace-stall/desktop-frame-layout-target`, with
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` and
`CARGO_INCREMENTAL=0` to keep the isolated build's disk use bounded.
The final log is
`artifacts/workspace-stall/desktop-frame-layout-isolated-tests.log`.
The final command passed: **25 tests, zero failures, zero ignored** (20 unit
tests and 5 native integration tests). The rustdoc target also succeeded with
zero doctests. Nine tests are new in this increment. Existing vendored and
dependency warnings remain; the changed desktop media code adds no warning.

An earlier shared-target run passed all 25 unit/integration tests, but its final
rustdoc invocation could not find dependency artifacts after another worktree's
build began using that target. Its failing command and output are retained in
`artifacts/workspace-stall/desktop-frame-layout-tests.log`; the independent
target avoids this cross-worktree artifact race.

No user-visible UI, Android code, dependencies, or third-party assets are
changed. This checks native frame handling; it does not assert compatibility
with additional pixel formats, hardware decoder buffers, or negative strides.
