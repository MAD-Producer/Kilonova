# Precise layer clip intervals

Project format 15 adds optional `Layer.subframe_range` with finite `in_frame`
and `out_frame` values in composition frames. Sampling uses the exact half-open
interval `[in_frame, out_frame)`. The existing integer `timeline` covers this
interval: `in_frame=floor(exact_in)` and `out_frame=ceil(exact_out)`. These integer
bounds remain useful for frame indexing and compatibility; they do not define
visibility when a precise range exists. Both bounds stay within −36,000..36,000.

```json
{
  "timeline": {"in_frame": 0, "out_frame": 21, "offset_frame": -6},
  "subframe_range": {"in_frame": 0.25, "out_frame": 20.75}
}
```

```json
{"op":"set_layer_range","composition":"comp-main","object":42,
 "in_frame":0.25,"out_frame":20.75}
```

The command preserves offset, transforms, keys, effects, source resources and
references. Integer endpoints also retain precise sampling mode. Invalid,
nonfinite, locked or unrecoverable source intervals fail atomically. Creating a
precise range promotes the document to format 15. Older integer documents keep
their stored representation and sampling behavior until the precise command is
explicitly used. Split and FPS operations retain precise mode even when an edge
lands on an integer, preventing one-sample holes or overlaps at mixed frame rates.

`move_layer_clip` remains an integer-frame translation. It shifts both exact
bounds and the integer offset together, preserving local animation. Legacy
`trim_layer_clip` preserves an exact edge when its covering integer edge is
unchanged; changing an edge explicitly sets that edge to the requested integer.
Use `set_layer_range` to set or snap exact endpoints directly. Integer-frame
splitting partitions the exact range and preserves the other two endpoints.

FPS changes with `preserve_seconds` scale precise bounds without rounding and
recompute covering bounds. Existing key/offset FPS conversion policies continue
to apply. Shortening still uses explicit reject/trim settings; trimming caps the
exact end and keeps retained animation. Precomposing and copying retain ranges.

Audio ranges with precise endpoints start/end at the ceiling of their position
on the 48 kHz output sample timeline, matching continuous half-open time. Nested
composition origins retain their rational timebase. Integer-only audio retains
its previous boundary convention. Source audio duration validation uses decoded
sample count/rate, including submicrosecond duration precision.

Capabilities: `capabilities.layer_timeline.subframe_ranges` declares `supported`,
`project_format`, `command`, `field`, bounds and `offset_precision`. Snapshot
`timeline_layers[].subframe_range` mirrors stored precision. Preview, nested
rendering, source-image queries, image prefetch, audio and frozen exports use
the exact interval. Android draws the exact edges and accepts decimal values in
the clip trim editor; moving and keyframe editing remain frame-based.

This capability covers clip bounds. Fractional layer offsets and fractional
stored key times need a separate timebase capability and are rejected by their
existing integer contracts.
