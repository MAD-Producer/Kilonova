# Independent 2D receivers beside calibrated spatial geometry (#87)

Status: local actual-GPU acceptance passed, 107 distinct tests, zero failures
or ignores. Main alignment and exact-head maintained-desktop CI remain separate.

The shared-geometry selector previously rejected every spatial run when any
Scene layer had a mask, matte or hidden output. A separate ordinary 2D masked
overlay could therefore prevent a lower calibrated image from rendering at all.
Masks, matte dependencies and hidden outputs now form local run boundaries.
Matte consumers and every referenced source remain isolated, including visible
3D sources: a receiver still needs its source's independent raster.

Scene effects, adjustments and nested compositions retain their
conservative Scene-wide exclusions. Applying a mask or matte to calibrated
geometry itself, or consuming it as a matte source, retains the contextual
unsupported error and the previously completed front image. This does not
silently flatten an unsupported depth operation.

## Real failure and pixel proof

The unchanged parent selector fails both new functional cases with the original
object/source/phase error. The calibrated-matte-source rejection control passes
under both selectors. The final parent run is 1 passed / 2 failed / 0 ignored;
the desired selector passes all three new tests on the actual GPU.

The first green attempt exposed a mistake in the newly authored fixture: it
put a visible white matte source after its green consumer despite the project's
back-to-front draw order. The corrected fixture places the source behind the
consumer. Original expected green pixels, sample coordinates and every outside
coverage assertion remain unchanged. Both failed attempts and their exact inputs
are retained. No existing repository assertion, tolerance or timeout changed.

Actual pixels cover ordinary masks and visible/hidden, flat/spatial matte
sources. Spatial geometry is analytically projected onto the same screen
rectangle. Every interior pixel outside the receiver coverage equals the native
spatial baseline; the known receiver interior is independently green. The
retained PNG was inspected: its green corner remains separate from red/blue
depth occlusion. The existing depth, model, mask, matte and retained-raster
integration suites pass 84 distinct tests. A separate
compositor/BSP unit gate passes 2 tests.
Panel-style and clean-room provenance checks pass on identical product sources.

## Evidence and remaining scope

[Original failures, exact inputs, native pixels and final logs](data/depth-composition-boundaries-native-v1.zip),
SHA-256 `21f1c269d45d541cb279937d1ac365ded91c5ed6d56365f5f42de8c1bee10495`. All 54 entries were read back and verified;
build binaries are excluded.

No panels, UI tokens, dependencies or third-party assets change. Android is
retired and excluded. Effects/masks/mattes on depth geometry, nested depth,
exposure, intersecting transparency, DOF, sustained throughput, final #87
workspace/cross-platform closure and #82 runtime prerender integration remain
pending. This increment does not close either issue.

## Subsequent independent blend boundary

An unrelated non-normal 2D blend is now also a local boundary. The blended
receiver stays independent and preserves its authored position around the
spatial raster. Scene effects, adjustments and nested scenes remain conservative
Scene-wide exclusions; non-normal blending on calibrated geometry itself is
still explicitly unsupported and preserves the completed front on failure.

Two new actual-GPU tests establish known red and blue depth rays, then verify
Screen composition with independent encoded channel expectations in Linear and
sRGB blend spaces, both behind and in front of the geometry. The visible
receiver outside geometry must retain its exact known RGBA value. The first
fixture used 0.5 coverage and exposed a native midpoint quantization of alpha127
versus128. It now specifies byte coverage128/255, avoiding that tie. Expected
pixel values, original assertions and their bounds remain unchanged. This is a
fixture input correction, not a product alpha fix; all original failures remain
in the evidence. The corrected fixture fails under the old selector (1 passed,
1 failed) and passes under the new selector (2 passed, zero failures or ignores).

Fresh related regression passes 86 distinct tests; the
separate compositor gate passes 2. Final local
combined acceptance is 88 distinct tests, zero failures or ignores. Style
and provenance checks pass on the same source bytes. This supersedes the
earlier local test total above without adding overlapping executions.

[Blend parent failure, native renders, final logs and exact inputs](data/depth-composition-blend-native-v1.zip),
SHA-256 `a88db96b375b0b45ca4fc613690f8f3d283f21601d6a1fa6a629b1252e3c1d8d`. All 55 entries were read back and verified.
The source-bound evidence does not replace main alignment or cross-platform CI.
The remaining issue scope and exclusions above still apply, except that
unrelated non-normal receivers no longer disable the entire depth scene.

## Subsequent image-effect ownership and input boundary

Ordinary image-effect owners and every explicit layer image-input source now
remain independent rasters. Their presence no longer disables an unrelated
calibrated spatial run. Scene effects, adjustments and nested compositions
retain conservative global exclusions. Effects directly on calibrated geometry,
and consuming calibrated geometry as an external image source, remain explicit
errors and preserve the completed front. This does not flatten those semantics.

Three new actual-GPU tests cover green Fill pixels beside depth geometry;
Source/Effects input stages with visible/hidden and flat/spatial sources; and
both calibrated-owner/input rejection paths. A known blue displacement map
shifts input coordinates four pixels left. An independently known uncovered
left-edge pixel proves consumption of that source, while the receiver interior
remains green and every pixel outside its bounds matches the native depth
baseline. The manifest declares unchanged input output-bounds and zero padding;
the receiver must not expand. All source visibility and stage combinations run.

The first fixture incorrectly used Invert, whose declared contract rejects an
external image input. The second incorrectly expected displacement to expand
zero-padding bounds. Both failures and exact inputs are retained. Final fixtures
use the supported Displacement Map contract, require the uncovered edge, and
retain an explicit transparent outside-bounds guard. Existing repository tests,
assertions, budgets and timeouts are unchanged. The final old-selector control
passes its rejection guard and fails both functional cases (1 passed, 2 failed);
the desired selector passes all three. Native fixture pixels were inspected.

Fresh related regression passes 105 distinct tests and a
separate compositor gate passes 2. Latest local
acceptance is 107 distinct tests, zero failures or ignores. It includes the
ordinary effect and spatial-effect suites in addition to the earlier targets.
Panel-style and provenance checks pass against unchanged product source bytes.

[Original failed references, parent control, final native pixels and logs](data/depth-composition-effects-native-v1.zip),
SHA-256 `0de07aa5f8402019221d1a62a415786d92009d4b31befd90fd39166c24b28020`; all 75 archive entries read back and verified.
This supersedes the earlier local totals without adding overlapping runs.
Main alignment, cross-platform CI and the remaining issue scope are separate;
this does not complete calibrated effects, nesting, exposure, DOF or #87/#82.

## Maintained-main alignment

The complete local implementation was rebased onto main `cd49b72d7cdf03886185b82ba6229220326ac208` before
this acceptance. Fresh source-bound tests include the incoming image active-set
implementation and scheduling module, alongside all nine related render targets
and compositor units. The gate passes 120 distinct tests,
zero failures or ignores; desktop FFmpeg/AI-depth compilation, panel-style
consumers and clean-room provenance also pass. Original and current source
hashes and raw output are retained. Earlier overlapping runs are not added.

[Main-aligned raw gate, native pixels and exact inputs](data/depth-boundaries-main-native-v1.zip),
SHA-256 `741601ec4d229a27315a00c46ee91f1b9de69063d53d7abd86a272274a6511e7`; all 57 entries read back and verified.

This targeted acceptance includes incoming renderer changes and verifies desktop
compilation of incoming UI code; it is not a new full-workspace or UI visual
acceptance. Own code changes do not touch panels. The file-backed video export
dependency was present as two separate local commits at gate start. After
PR549 merges, only the later composition commits belong in the next PR. The
dependency collapse must preserve byte-identical production/test inputs, or
affected checks must run again. Final cross-platform checks and remaining
#87/#82 scope still apply.
