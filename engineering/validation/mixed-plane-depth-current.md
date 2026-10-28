# Actual mixed model and opaque 3D plane depth

## Delivered behavior

A plain opaque 3D solid and an intersecting model previously rendered through
independent images. Authored layer order could place the entire plane above the
model, even where an independent world-space ray intersects the model first.
Compatible contiguous model/plane runs now enter one native depth pass using
their actual geometry, common camera and authored world transforms. Every
plane fragment obtains interpolated hardware depth. The implementation does
not flatten a raster and assign one invented layer Z.

The internal procedural plane has six immutable clip/color vertices, 192 admitted
GPU bytes per encode. No synthetic model file is parsed per frame. Its pipeline
cache has at most eight keys: four working color formats and two supported
sample counts. Cold pipeline construction emits Info start with request, source,
object, format and sample context; healthy frames add no file IO or routine
warnings. Full u64 object identity is preserved in invalid geometry errors.
Existing ScenePass failure suppression and typed resource refusals are retained.

The original public native ScenePass APIs are unchanged. Renderer supplies
planes only through the internal composition bridge. The original project and
owned model sources remain intact; the first actual model anchors the combined
raster even when a plane precedes it. Plane vertex bytes and actual draw/triangle
counts join existing statistics and admission lifetimes.

## Acceptance and original failures

Exact parent 1bb6a165 reproduces **0 passed, 2 failed, 0 ignored** in the two
initial regressions, before product edits. Both the authored-order pixel and
independent transformed-camera/world-ray oracle fail. The first implementation
attempt is compile-only: it sends full u64 object IDs to an old usize helper and
omits the new stats field in an existing literal. Those errors are corrected in
product code. The original two tests then pass without changing their assertions.
The expanded gate's first attempt is also compile-only: a new budget fixture
retains a renderer borrow. Cloning the existing shared budget handle corrects
that test setup without changing an assertion, tolerance or budget. Every
attempt, original cause, executed input and controller is retained.

Final actual native acceptance: **397 passed, 0 failed, 0 ignored across 11
targets**. It includes the renderer library, 25 model integrations, 45 native
scene tests, ordinary and nested compositions, layer compositing/BSP, rasters,
masks, scene/spatial effects, exposure and multiple-camera compositions. Earlier
overlapping passes are not added to this number. Five new GPU regressions prove:

- Saved/reopened authored projects, both layer orders, source files removed
  after configuration, and real near-plane/near-mesh pixels at 1x and 4x
- Independent f64 world-space ray intersections under actual camera and parent
  transforms, checking conservative interior pixels on both surfaces
- Exact native raw bytes against independent isolated surfaces for Unorm8,
  Unorm16, Float16 and Float32, each at 1x and 4x
- Distinct animated geometry in two pending encoders submitted in reverse,
  abandoned commands, exact retained pixels and restored admitted capacity
- One-byte-short plane vertex refusal, unchanged output, exact recovery, full
  u64 invalid-object context, ordinary zero-area geometry and depth retirement

Desktop FFmpeg/AI-depth compilation, provenance and panel-style validation pass.
This checkpoint is based on the existing accepted model-block commit. Its
execution hashes identify that exact source; it does not claim later main
changes, a new full-workspace run, cross-platform results or model inference.

## Remaining boundaries

At least one model is required in a shared run. Pure-plane scenes retain the
existing exact BSP path. Ordinary 2D layers and incompatible sampling end runs;
transparent or textured planes, layer effects, masks, mattes, adjustments,
nesting and non-normal blend scenes retain their existing receiver path.
Transparent mesh primitives still use their existing global primitive-center
ordering; this is not exact intersecting transparency or OIT. Those extensions
and durable current-project calibrated depth routing remain open under #87/#74.
This supersedes only the mesh/opaque-solid-plane limitation in the earlier
model composition checkpoint. It does not close either issue.

No UI/panels, styles, dependencies, third-party assets or compatibility changes
are introduced. Android is retired and excluded. The delivered movie is not
rerendered. Existing assertions, timeouts and tolerances are unchanged.

## Evidence

[Actual red/green attempts and the final native gate](data/mixed-plane-depth-native-v1.zip):
SHA-256 `e61e55171d93f572f42f29911eefe709e9eef4088848ecd8350310af8d8e359e`, 844,283 bytes,
70 entries. Every entry is read back and verified against the
included manifest. Binaries are excluded; complete executed source hash maps,
changed product/test inputs and original failures are retained.
