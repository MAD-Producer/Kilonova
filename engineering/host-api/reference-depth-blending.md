# Registered reference-depth blending

The model API combines explicitly calibrated AI depth with a reference field
before either becomes native 3D geometry. It does not run inference, estimate
motion, register cameras or stabilize a video automatically. No new proprietary
depth node, panel or effect parameter is introduced.

## Admission and arithmetic

Construct `DepthBlendInput` for both resources. Raw relative fields require a
sample-bound `RelativeDepthCalibration`. Metric and inverse-metric fields use
their declared physical units; ray-distance fields additionally require finite,
positive source-grid ray cosines. Both inputs must declare one common
`DepthProjection` camera and world-unit basis. A two-dimensional image warp does
not convert Z from a different camera or repair parallax.

`DepthRegistration` binds target and reference source IDs, revisions, exact
rational PTS and original image extents. `same_sample` is available only for the
same sample and extent. Other cases require a supplied target-to-reference
pixel homography through `new`, including intentional identity mappings between
different times. Invalid matrices, crossing projective horizons and stale
bindings fail. Values outside the registered reference crop remain missing.

The output grid and source identity follow the AI input. Each output texel centre
maps through its declared crop, quarter-turn and mirror back to the original
image, then into the registered reference and optional mask. Sampling is nearest;
there is no hidden resize pass or edge extrapolation. The formula is:

```text
factor = global_weight * linear_mask       // mask is 1 when absent
depth_m = mix(reference_camera_z_m, ai_camera_z_m, factor)
```

All weights must be finite in `[0,1]`. `DepthWeightMask` owns a linear scalar
plane and optional independent validity; it does not read RGBA, apply sRGB
conversion, use image alpha or threshold confidence. Intermediate weights need
both contributing depths. Exact zero reads only the reference; exact one reads
only AI. A zero global weight also skips the mask. Missing required data produces
an invalid output sample, not a guessed endpoint or fallback depth.

`DepthBlendOptions` supplies the output byte budget and the identity of the
authored derived procedure. Do not reuse an input inference model's hash as if it
identified the derived procedure. Both original descriptors and calibrations
remain in `DepthBlendProvenance`, together with registration, mask geometry,
camera, units and weight. Provenance metadata is not a copy of the input planes.

The result is an immutable R32Float camera-Z resource in metres, with independent
validity and no fabricated confidence. Positive values that overflow or underflow
R32Float fail with their output pixel index. The retained output payload is five
bytes per pixel, bounded by both the caller and the resource-wide hard limit.

## Compiler and native consumers

`motion_graph::scalar_presets::masked_lerp` is an optional ordinary group of
existing scalar Multiply and Lerp nodes, with four explicit ports: reference,
value, factor and mask. It compiles through normal and editable paths. The
compiler's constant folding and the model blend share the generic scalar kernel;
generated GPU arithmetic uses the existing `mix` implementation. A shader's zero
ALU-only sample-site count does not mean the depth operation has no input reads.

Canonical camera-Z output can be passed to `ProjectedDepth::from_camera_z` for
native hardware-depth seeding or `DepthSurface::from_camera_z` for existing
layer-local mesh construction. These entry points reject raw relative fields and
ray distances. They preserve the existing geometry, validity, camera clipping,
allocation limits and normal unknown-sample policy. The caller must retain the
authored camera/source guards when admitting results to a scene.

`DepthBlendMetrics` counts logical depth/mask/validity/ray-plane accesses and
retained output bytes. These are not measured GPU bandwidth or total process
memory. Native projected fields additionally retain depth plus validity and
incur their existing GPU upload/uniform cost. CPU timings and GPU submission
timings must be reported separately; submission duration is not GPU duration.

## Video and ownership boundaries

A fixed reference may be reused for a new PTS only with an explicitly supplied
registration bound to that particular target sample. A stale registration
cannot silently follow a moving subject. Incorrect authored registration can
still produce a ghost: identity reuse is deliberate authoring, not evidence of
correct alignment. This feature makes no claim that fixed-reference blending
necessarily reduces flicker or estimates unknown scene geometry.

Model errors retain their causes for the owning worker/session to report through
the existing default-level logging infrastructure. Invalid/out-of-crop texels are
normal missing samples; no per-pixel warnings or file I/O are added. Source
admission, history/persistence and UI authoring remain responsibilities of the
calling pipeline. Android is outside the maintained acceptance scope.
