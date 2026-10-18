# Noise primitive delivery

## Scope and status

This batch implements the second of the six requested primitives: 2D Value and Perlin Noise. Gradient was delivered previously. Generalized Blend, HSV Adjust, Color Ramp and UV Transform remain pending. The broader node-editor usability goal remains active, including reusable groups and instance parameters, naming, presets, conversions, gestures, diagnostics expansion and NodeSpec. No claim is made to have simplified all advanced examples or completed the entire audit.

Implementation: `73c856afff4659edff2b3a1c8cdb5df9cdb2a089` on `codex/node-noise`, based on `ec5ddd098e8bea0384f097ce8c5845360e4c937a`. Evidence: `E:/Dev/aem/artifacts/node-noise/validation-summary.json`. The executable SHA-256 is `245f7af04e480e117b8365140fcf1286649f0666eeb1cdcb4cfcc9c6dcefb8fb`.

## Behavior and boundaries

- Value and Perlin variants create one Noise operation; both mode and octave count have independent, explicit dropdowns, preserving each other, input defaults and wires through undo/redo
- One to four octaves are statically unrolled, doubling frequency and halving weight, normalized by total weight; invalid counts fail with node context
- Explicit normalized UV and optional stored/wired UV offset, frequency scale and seed produce Float factor and opaque grayscale Color outputs
- Scale is clamped to 0..4096, seed to 0..65535 then floored, and each octave domain to +/-1,000,000 before integer conversion; octave seeds advance by 7919
- An independently authored wrapping-u32 lattice hash drives scalar values or eight 2D unit gradients with quintic interpolation; this is not a copied reference 3D permutation-table implementation
- The original Three-node Noise example contains UV, Noise and Color Output, two wires, one pass, no texture samples or added resources; existing catalogue order is preserved and its count increases from 17 to 18
- Sources roundtrip, packages reopen through the actual effect host and GLSL ES 300 validates; Android device comparison and sustained performance remain pending

Equal inputs are deterministic on the tested implementation, without a cross-device bit-identity promise. Variation requires an explicit animated connection; there is no hidden clock. This is not 3D/volumetric, Simplex, filtered or guaranteed seamless noise. High frequencies can alias. The user's subsequent output-antialiasing report is a separate pending task; Noise delivery does not claim to resolve it.

## Verified evidence

| Check | Result |
| --- | --- |
| `cargo check -p motion-desktop --offline` and debug build | Passed; existing 21 build warnings retained |
| `cargo test -p motion-desktop --offline -- --test-threads=2` | 527 passed, 0 failed, 1 existing native-audio-device test ignored; 161.21 seconds |
| `cargo test -p motion-graph --offline -- --test-threads=2` | 56 passed, including 15 GPU tests and 6 new Noise tests |
| `cargo test -p motion-ui --offline --test style` | 6 passed |
| Python tooling suite | 53 passed, 1 existing skip |
| Style-consumer guard, diff whitespace, panels diff | Passed; no panels files changed |
| Native acceptance, minimum/en | 55 checks passed; 83.42 seconds |
| Native acceptance, minimum/zh | 55 checks passed; 69.75 seconds |
| Native acceptance, default/en | 55 checks passed; 106.16 seconds |
| Native acceptance, default/zh | 55 checks passed; 90.56 seconds |

Each native case unsets MOTION_LOG, uses an isolated saved layout and leaves stdout empty. The actual node floating window is 900x600 for minimum cases and 1280x800 for default cases, with the default main window at 1600x1000. The prior 48 checks remain, with seven Noise checks added. The user application's process and executable are preserved.

GPU tests compare all pixels for both modes and all four octave counts to an independent f64 weighted-corner CPU oracle, including negative lattice coordinates and fractional seed flooring. Other GPU checks cover repeated renders at different times, changed seeds, scale/seed bounds, zero scale and the domain clamp against independent constant expectations. Unit tests cover export/source reopen, factor preview, required UV, known pixel-coordinate rejection, typed defaults, invalid counts, unknown modes and unreachable-helper exclusion. Desktop tests exercise both dropdowns, their distinct labels and preserved values/wires, independent undo/redo, editable defaults, catalogue creation and wired-default precedence. Existing behavioral assertions remain; catalogue-count assertions reflect the new example.

Native acceptance operates the real mode and octave dropdowns, checks distinct GPU fingerprints for Value, Perlin and four octaves, edits the seed through native numeric input, saves/reopens source and verifies undo restores the prior GPU image. Project revision and layers are checked unchanged after the combined Gradient/Noise sequence. Actual images are `node-smoke.noise-{value-ui,modes,perlin-ui,octaves,layered-ui,edited-ui}.png` in each case folder. Chinese/English mode and octave menus and both window sizes were visually inspected; no synthetic UI is used.

## Design and provenance

No panels files, dependency, external asset or Adobe content is added. Controls reuse the existing Color category and socket-default/popup styles; their zoom behavior remains as previously requested. Mathematical reference and independent implementation scope are recorded in ATTRIBUTION.md. All DESIGN.md section 0.7 remediation rows are retained outside this batch, with no fix claimed. Section 12 evidence covers this workflow, not all panels or all DPI settings.

Known limits retained for follow-up include small fit-all text at minimum window width and the existing narrow inline-choice label column, which ellipsizes English Octaves. The existing U+2194 missing-glyph diagnostic and machine-specific missing Epic Vulkan overlay JSON errors remain visible on stderr; the previous Gradient evidence contains the same glyph issue. These are not suppressed or represented as fixed. Initial test infrastructure failures (page-file pressure, a reused publication path and an out-of-range boundary fixture) did not justify changing product validation or weakening assertions; final accepted logs and current source fingerprints identify the verified result.
