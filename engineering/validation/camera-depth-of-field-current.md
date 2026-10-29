# Native camera depth of field checkpoint

This is one bounded #87 increment, not final issue or #82 acceptance. The initial
1,034-test related gate used main `3865b0feca1acc65b8a5d0f96a0ca9684ee26beb`
plus the two accepted shared-matte commits. Own #556 subsequently passed all
five required checks and merged as `e4d6e1425f5440009ad8234230efc9c051b36544`,
with the actual squash message verified in Chinese. This branch was then
rebased onto that main. All source/configuration hashes remained byte-identical
to the initial gate. The portable-aperture refusal discovered during audit
requires a subsequent source-bound gate before publication. No other agent's
product tree or PR was changed.

## Saved and sampled contract

`Camera.depth_of_field` is an optional authored aperture. Absent or zero diameter
retains the existing pinhole path. Focus distance and entrance-pupil diameter
are explicit composition-world units, not inferred metres, millimetres, AE
units, or raw relative AI samples. Both are independently sampled tracks in
the selected camera's local clock. A bounded maximum radius is measured in
output pixels. Camera/project serialization owns these values; imported camera
sampling carries the same authored aperture, and observer views replace it.
No new UI or general property/keyframe editing commands are claimed.

The aperture is geometric: for axial world distance z, focus F, diameter A and
vertical projection zoom Z in output pixels, the circle radius is
`min(max_radius_pixels, A / 2 * Z * abs(1/F - 1/z))`. This follows convergence
of aperture rays at the selected focus plane. It does not infer f-stops from
the horizontal film gate. Radius is bounded to 0.5..16 pixels and the gather
has a fixed maximum of 49 taps; this is not ray-traced bokeh or hidden-surface
reconstruction. Near focused samples are excluded from a defocused background
pixel's gather to avoid dragging a sharp foreground into that background.

## Actual native composition

Single models, compatible shared meshes, calibrated surfaces and ordinary
spatial planes enter the common-camera geometry pass when aperture is enabled.
The resolved color is private to each encode, before final SDR conversion.
The aperture reads the pass's actual hardware depth, reconstructs world
position with the inverse common-camera projection, and measures axial depth.
Four-sample depth is explicitly read sample-by-sample using nearest physical
coverage; no implicit hardware depth resolve is assumed. Source-local masks,
material textures and independently projected logical matte inputs are not
blurred a second time as if they were final scene color.

Ordinary authored 2D boundaries remain boundaries and paint sharply afterward.
With aperture disabled, ordinary pure-plane scenes retain the existing BSP
path. The pure-plane aperture output has an explicit planar anchor and screen
raster, rather than requiring a model/calibrated source or reprojecting a
screen-space raster as another spatial source.

Color stays in its native working precision and premultiplied representation:
Unorm8, Unorm16, Float16 and Float32. No SDR intermediate or unconditional HDR
clamp is introduced. Frame color and immutable camera blocks join the shared
admission owner before any output recording. Encoded frames retain independent
parameters/resources through reverse submission or abandonment. A validation,
capability or budget rejection keeps existing front pixels available.

## Explicit limits

Perspective cameras are currently required for enabled aperture. Blended
geometry has no authoritative single physical depth and is refused explicitly
until a transparent-depth policy exists; it is not silently rendered with
opaque depth. Nested/adjustment/exposure/effect or matte configurations outside
the existing shared-geometry eligibility likewise report an object and phase.
The ordinary paths remain available when aperture is disabled. General effect
materialization, intersecting transparency, arbitrary optical iris shapes,
large-radius/multilayer blur and portable-camera aperture execution are not
claimed. Android is retired and excluded. No dependency, panel, UI style or
third-party asset changes are included.

Portable packed plans and legacy plane geometry have no aperture execution
contract. They now reject an active sampled aperture explicitly, including a
child composition's independent camera, with composition/frame/packing-phase
context. The packed writer rejects before modifying caller bytes; legacy
parameter and vertex buffers remain intact. An absent or zero-diameter aperture
retains ordinary export. Native frozen PNG/movie output is a separate supported
path and does not go through this refusal. This is a functional export boundary,
not additional logging or an implicit bake.

## Acceptance evidence

Focused native tests have proved four precision formats, 1/4-sample depth,
focus/defocus, HDR, premultiplied clear, letterboxing, reverse pending frames,
exact release after abandonment, refusal/front preservation and recovery.
Actual composition tests distinguish the calibrated 80/120-world-unit surface
halves under animated focus, preserve crisp 2D overlays, and prove a one-model
scene cannot bypass aperture. The first frozen-output run exposed the ordinary
plane anchor gap; its failure is retained, and the fix preserves that test's
original assertions.

The portable audit reproduced successful exports with silently omitted aperture
in both the actual packed writer and host entry points. The first nested test
fixture hit the existing flat-only precomposition command boundary; only fixture
construction was corrected, without changing an assertion. The second red run
then reproduced all three intended failures, including a successful bundle from
a pinhole parent containing an aperture child. Original red sources and output
are retained. The subsequent broad gate must execute these unchanged assertions.

Frozen PNG pixels match native output exactly in all four precisions after
editor closure. Durable MP4 restore/export retains the animated focus; decoded
endpoints match their native references within the declared codec tolerance.
The source-bound related gate passed 1041 distinct tests, zero failed
and zero unexecuted ignored tests. The existing manual history performance
fixture is ignored by the ordinary model/core invocation and was explicitly
executed unchanged with --ignored --exact; its one pass is included once.
Coverage includes full model/core libraries and integration targets,
ten render targets, full renderer library, host camera/export controls,
scheduler, frozen aperture output and the main screenshot-failure control.
Desktop FFmpeg/AI-depth compilation,
style consumers and provenance checks passed. Every Rust/configuration hash
remained unchanged after every stage. Focused tests overlap this gate and are
not additional distinct coverage. Initial focused fixture/build errors are
retained and are not claimed as product-red controls.

Final current-main full-workspace/maintained-desktop CI, normative comparisons,
long-duration real-codec depth ownership/playback and #82 actual prerender
cache integration remain separate outstanding work.

Receipt: `artifacts/pr-queue/native-camera-dof-regression-v2/accepted.json`.
Archive: `data/camera-depth-of-field-native-v1.zip`; CRC and every entry verified against originals.
SHA-256: `caadc34d37e24bd5101326bd6d374adaeea4c3a947ed8f6b0c14662a4742d0a7`.
