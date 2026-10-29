# Independent 2D receivers beside calibrated spatial geometry (#87)

Status: local actual-GPU acceptance passed, 86 distinct tests, zero failures
or ignores. Main alignment and exact-head maintained-desktop CI remain separate.

The shared-geometry selector previously rejected every spatial run when any
Scene layer had a mask, matte or hidden output. A separate ordinary 2D masked
overlay could therefore prevent a lower calibrated image from rendering at all.
Masks, matte dependencies and hidden outputs now form local run boundaries.
Matte consumers and every referenced source remain isolated, including visible
3D sources: a receiver still needs its source's independent raster.

Effects, adjustments, nondefault blending and nested compositions retain their
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
