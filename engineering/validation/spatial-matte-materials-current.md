# Pointwise materials for visible ordinary spatial matte images

This #87 checkpoint extends [calibrated material acceptance](calibrated-depth-materials-current.md).
It is based on own PR #554's exact head `f2428e1ce2e490d75a941eaff31d0bac308d6f4e`.
That PR's accepted source is unchanged while this dependent increment is checked.
Neither checkpoint closes #87 or implements #82's actual prerender cache.

## Supported boundary

A visible, image-backed ordinary spatial plane can retain a source-space mask
and implementation-proven Invert/Tint material while exclusively supplying
calibrated track-matte receivers. It keeps its original affine geometry and
physical depth in the shared pass. Its independent projected effects-stage
image supplies matte alpha/luma; the shared backdrop is never substituted for
that image. Layer opacity is applied once in each independent interpretation.

The existing proof checks selected native programs, external input absence,
source coordinate preservation and sampled alpha behavior. Source-local masks
precede effects. A mask-only image also receives a private admitted material.
Private texture bytes retain their existing source/global admission and command
ownership; they cannot be overwritten by another pending frame. Material effects
are removed only from the post-spatial authored output, preventing double use.

Native planes now explicitly mark a proven working-color material revision.
Only those revisions accept the current working format; raw ordinary and
calibrated source uploads retain their sRGB requirement. No shader is changed.
Ordinary-only scenes return immediately to their existing BSP/effects path.

This is deliberately an image-backed, visible, exclusively-calibrated-consumer
boundary. Ordinary or effect-image consumers still require separate aliases;
processed solids/vectors/models, video planes without calibrated ownership,
source track-matte chains, custom blends and nondefault temporal sampling do not
enter this new path. Unsupported combinations retain an explicit error and
completed front rather than flat overpainting. Existing rejection assertions
for processed ordinary solid sources and unproved Fill are unchanged.

## Evidence

The first new test attempt failed to compile because its module path annotation
was missing. That is fixture setup evidence, not a renderer defect. After that
annotation was corrected, an actual GPU control reproduced the old
`object=1 source=7 ... visible spatial matte source=91` refusal. The same
independent expected colors and tolerances passed after the implementation.

Three native tests cover:

- Four working precisions with an independent magenta luma calculation, a
  source-local half mask and half layer opacity; a far visible source cannot
  overpaint opaque blue, while moving it in front produces its independently
  calculated contribution; both positions are invariant to authored order
- Two pending material revisions submitted in reverse, an analytic identity
  blend, typed budget refusal with completed-front preservation, eviction and
  correct rematerialization
- Mask-only materialization, original object/source causes for unproved Fill,
  refusal when an ordinary consumer shares the visible source, unchanged front
  after both errors and recovery on the next valid frame

The source-bound related gate passed 473 distinct tests, zero failed
and zero ignored: ten native integration targets, the full renderer library,
host scheduler and main screenshot-failure control. Desktop FFmpeg/AI-depth
compilation, style-consumer and provenance checks also passed. Every tracked or
new Rust/configuration hash stayed unchanged after every step. Focused tests
overlap this gate and are not added to its distinct count. This is related local
acceptance on the stated dependent source, not full-workspace or cross-platform
acceptance.

## Remaining scope

Shared ordinary/effect consumers, non-image processed spatial sources, general
image effects and changed bounds, nested/exposure depth, intersecting
transparency, DOF, sustained real-codec/P95 measurements, final maintained-desktop
workspace/CI acceptance and the #82 actual render/cache runtime remain open.
Android is retired and excluded. No panel/UI, dependency or third-party asset
changes are included.

Receipt: `artifacts/pr-queue/spatial-matte-material-regression-v1/accepted.json`.
Archive: `data/spatial-matte-materials-native-v1.zip`; CRC and every entry verified against originals.
SHA-256: `fd482c506bd112b5fc79567006fef3ac75743108746b00dc467f3994d9851d8f`.

## Later shared-consumer checkpoint

[Shared independent matte input acceptance](shared-matte-inputs-current.md)
supersedes the exclusively-calibrated-consumer restriction for the supported
visible image materials. It adds ordinary consumers/intermediate parents,
retains physical geometry and authored 2D boundaries, and keeps external effect
image inputs explicitly unsupported. The earlier counts and rejection fixture
describe their historical checkpoint.
