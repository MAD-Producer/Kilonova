# Node output antialiasing

## Scope

The user's output screenshot showed a rough sphere silhouette and a stair-stepped
rotated rectangular background. These are two separate sampling problems. This
batch changes rendering, not node controls, panel layout, styling or gestures.
The broader node-editor usability audit and four remaining primitives are still
pending; this document does not mark that goal complete.

Base: `8a7cebd00ff1898dc12d1283ecf356eca302ad26`.
Branch: `codex/node-output-antialiasing`.
Evidence directory: `E:/Dev/aem/artifacts/node-output-antialiasing`.

Before editing, the other active work was checked. The separate
`fix/desktop-path-antialiasing` branch at `de27833` changes UI path strokes and
handles, not scene output planes. `feat/property-selection-context` at `4138feb`
changes property/path editing. Neither branch was modified or imported here.

## Diagnosis and changes

1. Existing coverage: `ng_fill` and `ng_stroke` already use `fwidth`; merely
   adding another smoothstep would not fix the observed output
2. Fixed: the final textured plane used single-sample triangle coverage. Pixels
   whose centers lay outside the rectangle could not run an edge shader. The
   native wgpu compositor now expands the original rectangle by a bounded pixel
   fringe before BSP splitting, retaining its original normalized UV boundary
3. Fixed: a shared fragment helper integrates a locally linear edge over a
   square-pixel footprint. It multiplies all premultiplied RGBA components by
   coverage, including the normal, masked and matte/blend plane shaders
4. Fixed: fringe sampling clamps to the actual used texel rectangle rather than
   unused space in a larger pooled scratch texture. Color and mask textures use
   their own texel dimensions, including the 1x1 solid-color texture case
5. Fixed: magnifying a 64-pixel SDF effect previously stretched its 64-pixel
   raster and its antialiasing footprint. Image-only effect layers now choose
   raster density from their projected footprint, while retaining logical
   positions, source regions, parameter values, clocks and shader packages
6. Preserved: BSP split boundaries do not acquire separate fades or overlapping
   fringe triangles. Existing geometry/output budgets still apply. Transfer and
   adjustment composition uses the working canvas pixel size for its fringe,
   even when that canvas is later presented into a larger target
7. Preserved: shared frame-plan version 9 and GLES vertex records retain their
   original unexpanded geometry. GLES plane programs explicitly retain legacy
   coverage until that renderer opts in with matching geometry support

Density uses stable 1/2/4/8 buckets above the existing base density and is capped
at eight raster texels per logical pixel. This can increase image-effect scratch
allocation by up to 64 times in area compared with a nominal 1x raster. Dimension
and capacity guards remain authoritative: guarded preview refuses an oversized
effect with its original layer/effect context instead of silently bypassing it.
Adjustment layers, precompositions and scene/sprite generators retain their
existing density rules. Minified preview density rules are retained.

The boundary calculation uses exact one-dimensional box integrals and a product
approximation where two edges meet; perspective derivatives are locally linear.
It is not an exact area integrator for every corner or extreme perspective.
Near-plane crossings, singular projections and pathological fringes retain
hardware clipping, with a finite fringe bound. Arbitrary hard discontinuities,
high-frequency Noise and extreme magnification beyond the density cap can still
alias. No whole-frame blur, multisample framebuffer or extra full-screen AA pass
is introduced.

## Measured failure and recovery evidence

The regression tests first ran against the original behavior and failed on actual
GPU captures, independently of a successful compile. The quad oracle clips the
projected polygon against each pixel box. The SDF oracle integrates an analytic
circle using 16x16 subpixel samples only at boundary pixels; it does not duplicate
the shader's derivative or smoothstep calculation. The circle baseline was
captured after the separate plane-edge patch and before density selection; its
SDF raster still had the original nominal resolution.

| Measurement | Before | After |
| --- | ---: | ---: |
| Partial-alpha pixels on the rotated graph-output rectangle | 0 | 509 |
| Rectangle aggregate absolute coverage error, pixel equivalents | 123.9218 | 0.6680 |
| 6.5x magnified circle aggregate absolute area error | 1299.3187 | 57.1227 |
| Magnified circle transition pixels farther than 1.5 pixels from its contour | 5647 | 0 |

The same area oracle also checks plain, axis-aligned fractional, masked,
Alpha-matted, Luma-matted, blended, half-opacity, perspective and reflected
planes. Observed errors in these cases range from 0 to 0.6785 pixel equivalents
after accounting for the Rgba8 interior quantization level. Opacity is separately
checked within one byte; this avoids treating the 0.5 conversion tie or polygon
roundoff as thousands of geometric edge errors. The mask fixture covers half the
plane and repeats under Alpha matte and Multiply paths, so sampling a constant
center value cannot pass. The first full render run exposed that regression;
independent color/mask texel clamps fixed it, and the existing mask/feather
assertions were retained. Its failed log is kept separately from accepted runs.
An enlarged transfer target is also compared to independent bilinear resampling
of its actual working-canvas alpha.

A magnified identity graph is compared with direct image display, using a varying
color/alpha source to check UVs and premultiplied filtering. Measured visible RGB
MAE is 0.1259 byte and alpha MAE is 0.0143 byte. The exported Soft Sphere Motion
example still animates, returns to identical pixels after a random seek, and
retains the same texture allocation across that sequence.

Artifacts include original captures in `before`, `before-density` and `after`,
`sdf-comparison.png`, `quad-comparison.png`, the actual sphere output in
`after/soft-sphere-node-output.png`, and a saved acceptance project with its
exported package in `after/acceptance-project` and `after/plugins`. Comparison
panels are assembled from those GPU captures; they are not fabricated UI.

## Verification status

| Check | Observed result |
| --- | --- |
| `cargo test -p motion-render --offline -- --test-threads=2` | 205 passed, 0 failed; 1 existing optional external-image probe ignored because `MOTION_IMAGE_SOURCE` is absent |
| `cargo test -p motion-graph --offline -- --test-threads=2` | 61 passed, 0 failed, including five new output-AA GPU tests |
| `cargo check -p motion-desktop --offline` | Passed; existing build warnings retained |
| `cargo test -p motion-desktop --offline -- --test-threads=2` | 527 passed, 0 failed; 1 existing native-audio-device test ignored; 110.47 seconds |
| `cargo build -p motion-desktop --offline` | Passed; standalone debug executable copied to the evidence directory |
| `cargo test -p motion-ui --offline --test style` | 6 passed, 0 failed |
| Python tool tests | 54 run, 53 passed, 1 existing skip; 31.438 seconds |
| Style consumer guard and `git diff --check` | Passed |

The eleven added tests comprise one render unit test, five render integration
tests and five graph GPU tests. Existing behavioral assertions were not weakened.
The public GLSL plane-program generation validates all three ES 300 programs;
that proves shader translation, not Android device rendering.

Native application acceptance passed all 55 checks in each of four cases:
Chinese/English at minimum (900x600) and default floating-node (1280x800)
window sizes, with the default main window at 1600x1000. Case durations were
80.84, 75.06, 86.62 and 114.16 seconds. Each case used this exact executable,
retained empty stdout and produced a default-Info file log containing command
start records. Checks cover actual graph rendering, parameter edits, grouping,
wires, diagnostics, source reopen, undo/redo, Gradient and Noise controls.

The saved sphere project was also opened in the normal Chinese application,
with actual window captures at 1600x1000 and 900x600 in
`native-sphere/{default,minimum}.png`. Both were visually inspected: the sphere
and rotated white plane are visible with smooth edges. Only the acceptance
process's window was captured, using `PrintWindow(PW_RENDERFULLCONTENT)` after
checking its PID. The initial foreground-screen capture attempt was refused by
Windows focus restrictions; it is not counted as accepted evidence. Normal
window acceptance exited successfully, left stdout empty, recorded Info start
lines and preserved the saved project's SHA-256. Other running applications and
their project/settings files were preserved.

The runnable evidence executable is `motion-studio.exe` with SHA-256
`5078949a8db6c8ae2460b62f3cc151c02b6a23ede9a27de64672e6e8ae4524f6`.
`launch-preview.cmd` opens the saved sphere project with isolated application
settings and layout. It does not overwrite the user's running application or
reuse its project. `validation-summary.json` records the final source revision,
source/image fingerprints, accepted logs and retained pre-fix failures.

## Design and provenance

No panels files, theme tokens, dependency or third-party asset is added. The
coverage math, GPU regressions, pixel-area oracle and test sources are authored
for Motion Studio; no Adobe binaries, resources or product assets were inspected
or copied. All DESIGN.md section 0.7 panel remediation items are retained outside
this rendering batch, with no fix claimed. Section 12 evidence applies to the
tested workflow, not all panels, all GPUs or all DPI settings.

Android device comparison remains unverified. The shared image-effect density
planner changes its schedule there too, but the GLES plane-edge AA path is
deliberately unchanged. Existing machine-specific Vulkan overlay loader errors
and font diagnostics are not suppressed or represented as fixed.
