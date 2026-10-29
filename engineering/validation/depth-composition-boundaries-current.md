# Independent 2D receivers beside calibrated spatial geometry (#87)

Status: local actual-GPU acceptance passed, 88 distinct tests, zero failures
or ignores. Main alignment and exact-head maintained-desktop CI remain separate.

The shared-geometry selector previously rejected every spatial run when any
Scene layer had a mask, matte or hidden output. A separate ordinary 2D masked
overlay could therefore prevent a lower calibrated image from rendering at all.
Masks, matte dependencies and hidden outputs now form local run boundaries.
Matte consumers and every referenced source remain isolated, including visible
3D sources: a receiver still needs its source's independent raster.

Effects, adjustments and nested compositions retain their
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
spatial raster. Effects, adjustments and nested scenes remain conservative
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
