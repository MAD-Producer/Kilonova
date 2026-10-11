# Stereo audio level envelopes

Project format 14 adds optional `Layer.audio_levels` for audio, video and ordinary
composition references. Each channel is an independent scalar `Track<f32>` in
decibels. Missing envelopes mean 0 dB on both channels. Existing static `volume`
and `muted` controls retain their behavior. Older documents remain unchanged;
creating an envelope promotes their format. The accepted host range is −192 to
+24 dB. This range is a host contract, not a verified claim about every AE version.

```json
{
  "audio_levels": {
    "left": {"value": 0, "keys": [
      {"frame": 0, "value": 0, "ease": "linear"},
      {"frame": 48, "value": -39, "ease": "linear"}
    ]},
    "right": {"value": 0, "keys": []}
  }
}
```

Key times are signed layer-local frames in stored data. Editing requests use
unsigned frames in the explicitly named composition and convert through the
layer's `offset_frame`. Independent channel keys support hold and the existing
temporal easing curves. Spatial tangents and separated XYZ tracks are rejected.
Curves can overshoot; the sampled dB value is clamped to the declared range.

```json
{"op":"audio_levels","composition":"comp-main","object":42,
 "action":{"kind":"set","channel":"left","frame":24,"value":-12,"animated":true}}
```

Actions: `set` (optional `animated`), `animate` (`enabled`), `delete_key` (`frame`),
`move_key` / `copy_key` (`from`, `to`), `curve` (`frame`, `easing`), `clear`
(removes both envelopes). All actions except `clear` name `left` or `right`.
They use the regular command, batching, gesture and undo/redo interfaces. Failed
requests preserve the entire document. Locked layers and unsupported targets
return errors. Audio-level expressions are not currently supported.

Capabilities are under `capabilities.audio.levels`, including `supported`,
`project_format`, `range`, `channels`, `unit`, `sampling` and `nested`. Android
reads the range from this response and reuses `NumericWheel`, timeline keys and
`CurveEditor`. The selected channel is `audio:left` or `audio:right`.
`sampledLayers[].audio_levels` returns the current composition sample's channel
values; stored tracks live under `project.layers[].audio_levels`.

The mixer evaluates envelopes at each 48 kHz output sample. It interpolates dB
first, then converts to amplitude using `10^(dB/20)`. Static volume multiplies
this gain. A composition reference's envelope is sampled at its own local time
and frame rate, then applied to all audio voices inside that instance. Child
envelopes retain their own clocks. Rational nesting offsets are preserved until
final envelope sampling. Mono sources map to both output channels and can have
different left/right gains. Clipping still occurs after summing all voices.

Trimming, moving, splitting, duplicating, precomposing and FPS changes retain
these tracks through the existing layer/time contracts. Mixers own frozen
documents, so later editing cannot change an export already in progress.
This capability preserves level envelopes; it does not add audio DSP effects,
time stretching or cross-property expressions.
