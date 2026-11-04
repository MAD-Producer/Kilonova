# Composition, media and shader clocks

This contract accompanies the rational-project adoption change. Its complete
desktop UI and final cross-platform acceptance are still pending; the primitive
clock PR alone does not enable rational projects in a released application.

## Project and command data

The current project format stores `fps` as a required canonical object:

```json
{"numerator":30000,"denominator":1001}
```

Both components are positive integers, reduced by their greatest common divisor.
Each reduced component is bounded by 1000000; composition rates must also lie
between 1 and 240 pictures per second. An integer clock uses denominator 1.
Missing components, zero, additional fields and numeric FPS values are errors.
There is no migration or implicit decimal alias: `29.97` means `2997/100`,
whereas the fractional broadcast clock is explicitly `30000/1001`.

`capabilities.composition` publishes `fps_type: "rational"`,
`frame_rate_wire_version: 2`, canonical `fps_presets` and the required timing
policies. The composition request envelope remains version 1; its envelope
version is independent of the current project format and the FPS field format.

Settings commands require `timing: "preserve_frames"` or
`timing: "preserve_seconds"`. The former retains authored frame coordinates;
the latter rescales them directly between the two rational clocks. Integer
keyframes round once to the nearest destination frame, with ties away from zero.
Colliding keys fail the transaction instead of deleting either key. Fractional
render coordinates are not rounded to an integer destination frame.

## Media boundaries

For a frame coordinate `f` and rate `n/d`, seconds are `f*d/n`. Use the clock
API's explicit rounding operation when crossing an integer boundary, rather
than accumulating a rounded frame duration:

- Output video PTS: nearest integer microsecond, ties away from zero
- Work-area and unretimed integer-clip PCM edges: floor of the absolute sample
  coordinate; precise fractional active intervals use ceil to select the first
  output sample inside their half-open range
- Imported clip end: ceil of the destination frame coordinate
- Nested composition sampling: preserve fractional frame coordinates

Native video sampling retains source PTS and variable frame durations. A drop
timecode label never removes a picture or changes a decoder request. MP4 output
uses a native rational video stream timebase and a 48000 Hz movie clock so that
audio tails are not shortened by the muxer's default millisecond movie clock.
Container edit-list video durations can still be quantized to that movie clock;
this is distinct from the exact native video packet PTS and duration.

Timecode uses explicitly selected non-drop or drop labels. Drop mode supports
`30000/1001` and `60000/1001`. Labels do not wrap at 24 hours in this editing
format. This is a textual editing/display contract, not binary LTC/VITC packing.

## Composition bundle version 2

The bundle is little-endian, starts with magic `0x4243534d`, and has a 32-byte
header followed by 80-byte node records in postorder. Validate the exact version,
declared lengths, node count, root index and referenced ranges before reading.
The existing per-node effect plan has its own version and validation rules.

| Byte offset within a node | Type | Clock value |
| --- | --- | --- |
| 56 | IEEE-754 f64 | Unrounded local composition frame |
| 64 | u32 | Canonical rate numerator |
| 68 | u32 | Canonical rate denominator |

Version 1's reserved denominator word is not a denominator-1 fallback. Reject
the old version explicitly. The writer's maximum bundle size remains 32 MiB.

## Shader precision boundary

WGSL `fx.clock` and generated GLSL use the same four f32 values: local seconds,
local frame, pass index and seed. They are sampled independently from the host's
composition clock for each evaluation, then converted to shader precision.
The shader does not advance a mutable frame accumulator, infer an integer FPS,
or reinterpret a drop label as a time coordinate.

These shader values are approximations, not an exact media clock. Do not infer
an exact rate by dividing frame by seconds, and do not generate decoder PTS,
audio boundaries or cache identities from f32 shader uniforms. Use the canonical
project rate or version-2 composition node for those operations. No new uniform
layout or shader-package SDK version is implied by the rational project field.

Android is retired and has not been adapted or validated for this contract.
