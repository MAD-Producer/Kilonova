# Compact editable node examples

## Scope

This batch rewrites the existing Soft Sphere, Soft Sphere Pseudo-3D Motion and
Floating Light Spots examples using typed input defaults, radial Gradient and
the existing named, typed group implementation. Sphere shadows also use Normal
Blend where both source colors are opaque. Public color interpolation retains
Color Mix, including its original alpha behavior. Noise is not inserted into
these smooth examples because it would change the intended result.

The branch includes the previous node-editor deliveries and the output
antialiasing fix for a single reviewable PR. Their separate completion records
remain historical evidence, with the source commits and limitations documented
in each record. Current main `f32c64d` was merged before final validation;
the property-selection changes were preserved. The root checkout and all other
worktrees were left intact. The open timeline-effect-properties PR was checked
before implementation and does not change these example sources.

| Example | Previous operations | Current operations | Visible root nodes | Public parameters |
| --- | --- | --- | --- | --- |
| Soft Sphere | 54 | 40 | 9 | 5 |
| Soft Sphere Pseudo-3D Motion | 121 | 96 | 15 | 10 |
| Floating Light Spots | 71 | 46 | 7 | 3 |

Operation counts include reachable flattened computations and input sources,
excluding group boundaries. Root counts include the group and final output;
they are presentation counts, not shader-cost claims. Groups can be opened to
inspect the implementation, edited, saved and reopened. Their external ports
carry explicit public-parameter, coordinate, time and resolution inputs.

Constant nodes have become typed socket defaults. Three light-center pipelines
compute XY drift together with Vec2 math, and radial Gradient replaces distance
chains. Existing Smoothstep and squared falloff remain, preserving the artwork.
The analytic sphere lighting, orbit, perspective, lift, rotating band and
height-dependent shadow retain their original contracts. Public parameter IDs,
defaults, ranges, units, order and animation flags are unchanged.

These are source v2 examples, using already-supported typed defaults and groups.
They remain independent editable copies when opened from the built-in library.
Existing user-saved graphs are not migrated. No dependency, external asset,
render pass, panel layout, style token or shortcut is added by this batch.

## Verification

The GPU reference sources in `crates/motion-graph/tests/support/legacy_examples`
are frozen repository-authored examples from `fb34584`. Their parsed documents
were checked against that commit, independently of the compact graphs.

The new GPU test exports and opens both actual MSFX packages, then compares
their captures: three examples x three canvas sizes x four parameter cases x
five seeks x two versions = 360 captures. Cases cover defaults, all scalar
minimums, all scalar maximums, translucent tone plus a keyframed parameter;
sizes are 64x64, 128x64 and 192x192. Seek order is 0, 15.5, 47.25, 290, 0 frames.
The largest observed RGBA channel difference is 1/255, within the explicit
one-byte comparison tolerance. Returning to frame zero must restore exactly the
same pixels within each version. Existing sphere motion and antialiasing tests
remain intact.

A desktop regression exercises group entry/exit, editing the real stored
Gradient falloff, preserving every public parameter, compiling, saving from
inside the group, reopening, and undo/redo after returning to the root.

Native acceptance extends the existing node smoke sequence with all three
compact examples: capture root and internal canvases, edit a nested Gradient
through its actual numeric control at 1:1 zoom, require changed GPU pixels,
save/reopen the source, undo, require exact pixel restoration, and return to
the original root source. The project must remain unchanged. CI checks these
reports and the nine real UI screenshots; previous assertions are retained.

| Check | Observed result |
| --- | --- |
| `cargo test -p motion-graph --offline -- --test-threads=2` | 67 passed, 0 failed |
| `cargo test -p motion-desktop --offline -- --test-threads=2` after layout correction | 566 passed, 0 failed; 1 existing native-audio-device test ignored; 151.08 seconds |
| `cargo check -p motion-desktop --offline` | Passed |
| `cargo build -p motion-desktop --offline` after layout correction | Passed; existing 21 warnings retained |
| `cargo test -p motion-ui --offline --test style` | 6 passed, 0 failed |
| Python CI-tool tests with existing PyYAML dependency path | 51 run, 0 failed; 1 existing platform skip |
| Provenance guard and style-consumer guard | Passed |

Source implementation commit: `4abb801e2224f8ca9ddf7c32d0bd2225821147c0`.
Final executable SHA256:
`a68e48f05022d4f7da86f068466056f85bb544099389856749e747b5494722ee`.

Final native acceptance passed all 66 explicit checks in each of four cases:

| Window / language | Size | Duration |
| --- | --- | --- |
| Minimum English | 900x600 | 117.31 seconds |
| Minimum Chinese | 900x600 | 103.86 seconds |
| Default English | 1280x800 | 105.22 seconds |
| Default Chinese | 1280x800 | 112.98 seconds |

The normal main window is 1600x1000. Every accepted run used the executable
fingerprint above, default Info logging, an isolated project/settings directory,
empty stdout and file logs containing non-Present command starts. Existing host
Vulkan overlay-loader errors remain visible on stderr (1172 bytes per case);
they were not suppressed or reclassified. Application and CI smoke timeouts
were retained. These are local Windows default-feature results; Linux/software
Vulkan, macOS and FFmpeg-enabled builds remain for remote CI.

Root, internal and edited screenshots were reviewed in all four cases. Example
filenames are `node-smoke.soft_sphere-root.png`,
`node-smoke.soft_sphere-gradient-edited.png`, and
`node-smoke.floating_lights-root.png`, under `native/{minimum,default}/{en,zh}`.
The default root fit-all view is 68% zoom; the minimum view is 39%. Numeric edits
are exercised at 100%. Small fit-all views emphasize topology and still require
zoom for reading detailed parameter values. Existing English draft-title text
in some Chinese views is retained for the separate naming/localization work.

The evidence directory contains the runnable executable, isolated launcher,
GPU pairs, native reports, original/rejected logs and a validation manifest with
commit, executable, source and image fingerprints. CI is not claimed passed by
these local results; its status must be read from the PR checks.

## Evidence and limits

Local evidence: `E:/Dev/aem/artifacts/node-compact-examples`.

The first catalogue run found missing unused color-input defaults on the new
Gradient nodes. Both defaults were supplied in the product examples; existing
validation assertions were preserved. The rejected run is retained separately.
Native screenshot review then identified overlapping Light Direction and Shadow
Strength parameter cards. A new test using actual drawn card bounds reproduced
that defect before the fix. Source positions now use separate column heights
for scalar, vector and color controls, and that assertion passes unchanged.
The initial native evidence was archived separately; its remaining run was
stopped against only the verified acceptance executable. It is excluded from
final acceptance. The full desktop suite was rerun after the layout correction.
The first Python check used an interpreter without PyYAML; the rerun used the
repository's existing validation dependency directory, without a new install.

DESIGN.md section 0.7 panel-remediation rows are retained: this batch changes no
panel drawing code. The PR diff against current main must contain no panels
files. The graph layouts are independently authored example positions, not new
UI metrics or a parallel style system.

These groups are embedded graph implementations, not reusable installed assets
or new instance-parameter infrastructure. The motion example still has ten
public controls to preserve its existing behavior; it does not meet a future
five-control preset budget. Full internal graphs remain complex and require
zoom/pan. The root organization hides implementation detail without claiming
that sphere projection and lighting computation disappear.

Three of the proposed six primitives are delivered: Gradient, Noise and Blend.
HSV Adjust, Color Ramp and UV Transform remain pending, as do search/naming,
preset assets, instance parameters, wire insertion and the broader editor audit.
This PR is a completed batch, not completion of the entire node-editor goal.
