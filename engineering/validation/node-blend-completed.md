# Color Blend node

## Scope and remaining work

This batch delivers the third of the six requested primitives, after Gradient
and Noise: a single color Blend node with 15 selectable separable modes.
HSV Adjust, Color Ramp and UV Transform remain pending. The wider node-editor
audit remains active, including reusable group assets and instance parameters,
search/naming, presets, wire insertion, explicit conversions and NodeSpec.
No claim is made that the whole editor or all advanced examples are simplified.

Base: `f3be44e0ef5d0380a30b8c6d2f1114921a17df48`.
Branch: `codex/node-blend`.
Evidence directory: `E:/Dev/aem/artifacts/node-blend`.

Other branches were checked before implementation. The recent property-selection,
layer-render-sampling and timeline-effect-properties work changes desktop property
workflows, not graph Blend. Their worktrees and the root checkout were preserved.

## Delivered behavior

- One Color-library entry adds a Blend node with editable backdrop/source colors
  and strength, already compilable without additional constant nodes
- Its existing inline dropdown selects Normal, Add, Multiply, Screen, Overlay,
  Darken, Lighten, Difference, Exclusion, Subtract, Divide, Color Dodge, Color Burn,
  Hard Light or Soft Light; names are independently localized
- Transfer calculations use straight colors in the graph's declared working
  space, then source-over composition. Strength is clamped to 0..1 and multiplies
  source alpha. Zero retains the backdrop; a transparent source retains it too
- RGB and alpha are bounded to 0..1. This is not an HDR or non-separable Hue/
  Saturation/Color/Luminosity operation. It reads two Color values, not implicit
  layers, arbitrary resources or a host framebuffer
- Straight and premultiplied graph contracts are supported. Existing Color Math
  Mix/Multiply/Screen operations retain their original semantics; saved graphs
  are not migrated to source-over Blend
- Modes retain wires, existing typed defaults and absent fallbacks. The first
  interaction test exposed that `set_operation` eagerly seeded missing defaults
  on unchanged inputs. The product fix seeds only new or differently typed
  inputs, preserving source on a same-schema mode change. Existing type-change
  and undo assertions remain in place
- Opening a long inline menu scrolls to its current selection. Row size, text,
  padding and popup styling continue to follow the same graph zoom and existing
  styles as the field; wheel scrolling and keyboard selection retain their roles
- A five-node Gradient and Noise Blend example uses shared UV, Gradient, Noise,
  Blend and Output. Its colors, mode, noise and strength are editable; no imported
  images, hidden clock, external assets or extra render passes are involved

The graph document supports the new explicit `blend` operation without changing
the v1/v2 formats or SDK 6 image-pass ABI. Older readers that do not know this
operation reject it; existing readers are not promised forward compatibility.
The helper is emitted only for reachable Blend nodes.

## Evidence and verification

Three graph tests verify all 15 mode roundtrips, actual package export/reopen,
GLSL ES 300 translation, port preview, typed ports and invalid inputs/modes.
An unused Blend must leave the existing graph's generated shader identical.

The GPU oracle uses f64 source-over arithmetic with independently expressed
transfer calculations. It checks 15 modes x 2 working spaces x 2 alpha contracts
x 6 color/alpha pairs x 5 strengths = 1800 actual captures. Cases include zero
and full alpha, translucent overlap, source-only/backdrop-only regions, transfer
boundaries and strength below zero/above one. Every output pixel is checked
uniform, with one-byte alpha and three-byte straight-RGB tolerances for the
existing effect/capture quantization. The test passed in the focused GPU run.
A second actual GPU test animates strength at a fractional frame, then seeks
back and requires identical pixels from the exported example.

Desktop tests operate the real menu in Chinese and English at 0.5x, 1x and 2.5x,
check current-selection visibility in a clipped long list, switch modes, compare
source defaults/wires and verify undo/redo. A separate test adds the library
entry, compiles it and checks that a connected strength input takes precedence.

| Check | Observed result |
| --- | --- |
| `cargo test -p motion-graph --offline -- --test-threads=2` | 66 passed, 0 failed, including five new Blend tests |
| `cargo check -p motion-desktop --offline` | Passed; existing 21 build warnings retained |
| `cargo test -p motion-desktop --offline -- --test-threads=2` | 529 passed, 0 failed; 1 existing native-audio-device test ignored; 170.20 seconds |
| `cargo build -p motion-desktop --offline` | Passed; debug executable copied into the evidence directory |
| `cargo test -p motion-ui --offline --test style` | 6 passed, 0 failed |
| Style consumer guard | Passed; no panels changes |

The first full desktop run correctly rejected a stale example count in PANELS.md.
The document now says 19; the existing documentation assertion was preserved.
That failed run and the earlier absent-fallback regression are retained separately
from accepted evidence.

Native acceptance passed all 62 checks in each of four cases: minimum English
(91.86 seconds), minimum Chinese (93.20), default English (145.52) and default
Chinese (132.19). Floating node windows were 900x600 and 1280x800, with the normal
main window at 1600x1000. Every case used this exact executable and default
Info logging, produced empty stdout and retained command-start lines in its
isolated file log. Existing Error stderr records remain visible.

The seven new native checks require the real five-node single-pass graph,
different GPU pixels after selecting Screen through the actual mode menu,
different value/pixels after editing Strength to 0.25, source reopen, exact GPU
restoration through undo and redo, and an unchanged project. The prior 55 checks
remain in the sequence. Existing smoke timeouts were not changed.

Actual images are `node-smoke.blend-{soft-light-ui,modes,screen-ui,edited-ui}.png`
under each `native/{minimum,default}/{en,zh}` folder. The first image shows the
complete graph. Subsequent captures use 1:1 zoom to inspect the actual menu and
strength field with the GPU preview beside them. Mode menus and edited views
were visually inspected across both languages and window sizes; these are
rendered application captures, not generated mockups. The user's running
application and project/settings files were preserved.

The runnable executable is `motion-studio.exe`, SHA-256
`43db64fdd921b4a41ea0c6a365203fb3595777d23b27c0739a68c87e1231100b`.
`launch-editor.cmd` opens an isolated project/settings directory. Choose Nodes,
Examples, then Gradient and Noise Blend to edit the new example. The launcher
does not overwrite an existing executable or reuse a user project.

## Design, provenance and limits

No panels file, theme token, dependency or third-party asset is added. New
controls reuse the existing inline choice and input-default presentation. All
DESIGN.md section 0.7 panel remediation rows are retained outside this batch,
with no fix claimed. Section 12 evidence covers the tested node workflow, not
all panels or all DPI configurations.

The mathematical reference is W3C Compositing and Blending Level 1, fixed
[2024-03-21 revision](https://www.w3.org/TR/2024/CRD-compositing-1-20240321/),
sections 6 and 10. Reference, license and independent implementation scope are
recorded in ATTRIBUTION.md. Add, Subtract and Divide are bounded arithmetic
extensions; no claim of complete W3C/CSS conformance is made. No Adobe binaries,
resources, product wording or artwork were inspected or copied.

GLSL translation does not prove Android device rendering or mobile performance;
those remain unverified. The default-feature desktop build is verified; the
FFmpeg-enabled build was not rerun for this graph-only change. Existing
machine-specific missing Vulkan overlay JSON
and U+2194 missing-glyph diagnostics are retained, not suppressed or called fixed.
The previously delivered desktop plane/output antialiasing remains a separate
rendering change; high-frequency procedural content can still alias.

Visual follow-ups retained after inspecting the native captures: fit-all text is
small for the complete five-node example at 900x600; the 1:1 mode/strength view
is legible. Clipped choice menus support wheel and arrow keys and reveal their
current selection, but have no explicit scroll thumb. The draft's title strip
still uses its English name in the Chinese UI. These belong to remaining
navigation/naming work, not completed fixes in this batch.
