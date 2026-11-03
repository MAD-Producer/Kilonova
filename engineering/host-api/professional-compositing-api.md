# Professional compositing API: scope and executable camera example

This matrix describes the current host/model interfaces, not a percentage of
another application's feature set. A source-defined capability, an executed
component test and an accepted professional shot are different evidence levels.
The new native example below covers the procedural camera row only.

| Capability | API and semantics | Boundary | Evidence location |
| --- | --- | --- | --- |
| Source masks | `mask` actions; paths and coverage live in source-pixel coordinates before the layer effect chain; animated opacity, feather, expansion and path geometry use the layer clock | A source mask does not select a separate composited layer; an open/disabled/None mask retains data without cutting coverage | `motion-core/tests/layer_masks.rs`; model `masks` module |
| Track matte | `set_track_matte` binds a stable source layer ID in the same composition; Alpha, Alpha Inverted, Luma and Luma Inverted use source coverage; `hide_source` controls ordinary source compositing | A hidden source can still supply a matte; missing sources, self references and dependency cycles fail; this is separate from source masks | `motion-core/tests/track_mattes.rs`; model `compositing::validate` |
| Blend transfer | `set_layer_blend` selects an existing `BlendMode` and Linear or sRGB blend space | The transfer follows the existing premultiplied composition path; the API does not promise equivalence to a different application's similarly named mode | Model `compositing::transfer`; renderer compositing tests |
| Precompose | `composition` / `precompose` moves a contiguous selection and all its attributes into a child composition over the full composition range | Flat selections only; no collapse transformations; cross-selection matte, image-input and parent dependencies are rejected instead of silently severed | `motion-core/tests/compositions.rs`; model `Project::precompose` |
| Camera | Camera layers, Position/Orbit modes, existing lens/transform tracks; active camera is the topmost enabled camera in range | Stored source camera and free observation camera are separate; unsupported imported-camera edits retain their existing rejection rules | `motion-core/tests/multiple_cameras.rs`; new public-host native camera example |
| XYZ separation | `separate_dimensions`, `set_component`, axis-specific `animate`, and existing `merge_dimensions` for accepted vector properties | Scalar properties reject separation; merging retains existing compatibility/error rules and does not resample unrelated keys | `motion-core/tests/separate_dimensions.rs`, `merge_dimensions.rs`; new native example animates only Y |
| Effect parameters | Stable effect instance and parameter IDs; actual schema publishes Numeric, Hold and Color Curve LUT support | Schema support does not imply an unlocked/enabled instance or available shader package; resource bindings are not continuous tracks | [Parameter contract](effect-parameter-animation.md) and actual host tests |

Existing component tests are listed so maintainers can locate coverage; their
current execution status belongs to the validation record, not this table.
The matrix does not certify roto, green-screen/spill, screen replacement,
complex matte/precompose, relink or final output for arbitrary footage.

## API fragments

These fragments assume the named objects already exist and are independently
applicable examples, not one batch to apply to the camera shot:

```json
{"op":"mask","object":7,"action":{"kind":"animate","mask":1,"property":"opacity","frame":0,"enabled":true}}
{"op":"set_track_matte","object":7,"matte":{"source":8,"mode":"alpha","hide_source":true}}
{"op":"composition","action":{"kind":"precompose","objects":[7,8],"name":"Flat pair","range":"composition"}}
{"op":"separate_dimensions","object":7,"property":"position"}
{"op":"animate","object":7,"property":"position","axis":"y","frame":0,"enabled":true}
{"op":"set_component","object":7,"property":"position","axis":"y","frame":35,"value":80}
```

The precompose fragment requires a contiguous eligible flat pair. It is not a
workaround to precompose the three-dimensional camera example, which must keep
its camera and spatial planes in the same composition under the current API.
Command frames are composition coordinates; the host maps them to stored local
tracks using the existing layer clock/clip-offset rules.

## Executable native camera shot

[`professional_compositing.rs`](../../crates/motion-desktop-media/tests/professional_compositing.rs)
uses the public host command parser and desktop platform. It authors a camera,
three solid planes, independent camera/rotation/Y-axis animation and built-in
brightness/contrast keyframes. Its image size is 320x180, its clock is exactly
30000/1001, and its duration is 36 frames. All source content is independently
authored in this repository under MIT; no external image, video, font or model
is used.

The far blue plane is deliberately later in the painter-order stack than the
near coral plane. With 3D disabled, blue must cover the center. With 3D enabled,
coral must cover the center because of spatial depth. This independent control
guards against accepting identical outputs from two equally flattened paths.

The same real API batch must undo completely in one step and redo exactly.
Saving/loading the project and creating another Session must preserve four
headless GPU RGBA renders, including the fractional frame 17.5. These samples
are compared byte for byte with independently rendered frozen PNG plans after
the live camera is changed, the near plane is deleted and both sessions close.
The frozen movie also renders after those live changes and session closure.

Every integer movie frame is decoded by the native desktop decoder and compared
with its corresponding headless GPU reference. MP4 average frame rate, every
video packet PTS/duration and the packet count are checked by exact integer
cross multiplication. Lossless PNG equality and the lossy movie comparison are
separate assertions. The latter permits RGB mean absolute error <=3 per frame;
YUV420 chroma edges can still have substantially larger individual errors, and
the report retains the maximum as well as the mean. This budget does not prove
pixel-exact movie export or perceptual quality for arbitrary footage.

Run the actual GPU and FFmpeg target together with the existing host contract:

```sh
cargo test -p motion-host -p motion-desktop-media --test layer_time_mapping --test professional_compositing --no-default-features --features motion-desktop-media/ffmpeg,motion-desktop-media/gpu-tests --locked --offline -- --test-threads=1
```

FFmpeg development/runtime setup is the same as the existing desktop media
tests. The GPU test is not silently accepted if disabled: the acceptance gate
requires zero ignored tests. To retain media and editing scripts, set
`MOTION_PROFESSIONAL_API_ARTIFACTS` to an output directory before running. It
contains the initial project, the applied JSON command batch, the saved editable
project, the later live state, headless/frozen PNGs, the 2D control, all 36 movie
reference PNGs, the actual MP4 and a comparison receipt. Apply `commands.json`
to `initial-project.json`; the saved `project.json` already contains the edits.

These are actual native headless renders and exports. They are not screenshots
of a desktop window, independently produced image goldens, photographed-shot
acceptance, compiled-node-graph proof, or acceptance of other working precisions.
The [validation record](../validation/professional-api86-parameter-contract-current.md)
tracks those remaining requirements.
