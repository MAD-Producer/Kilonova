# Semantic images and working precision: current delivery ledger

This is the active acceptance ledger for issue #72 on top of the merged #61
resource-admission and #67 physical scratch-lifetime implementations. Historical
experiments are not delivery evidence for the current source. The issue remains
open: final comprehensive acceptance remains pending. The project-setting UI
now has native interaction and screenshot evidence. Ordinary/exposure surface previews, plugin editor
preview, desktop UI/node screenshots and Host/frozen PNG/MP4 exports now use
explicit SDR output.

## Scope and status

| Item | Current implementation | Status and remaining limits |
| --- | --- | --- |
| 72-S1 | Versioned color/alpha, coverage, normalized/unbounded weight, relative/linear depth, signed XYZ normal and scalar/vector data contracts; 13 R/RG/RGBA formats; four precisions | Implemented; unbounded and signed semantics require floating storage, and normal's padded fourth channel is ignored rather than alpha |
| 72-S2 | Enabled-device format/usage/filter/blend snapshot and same-precision channel padding | Implemented; optional features are never inferred from adapter reports alone |
| 72-S3 | Before-allocation capability validation, including 4-sample vector attachments and resolve | Implemented; unsupported working modes return a cause instead of reducing precision |
| 72-S4 | Physical effect descriptors, same-frame layer snapshots, accumulation, masks, mattes, ordinary nested compositions and vector/group rasters retain working precision | Implemented; actual pixel tests cover the named paths below |
| 72-S5 | Exposure sample/mean/resolve and retained/nested exposure capacity use actual working storage | Implemented; floating sample/mean/resolve tested; a comprehensive retained/nested temporal precision golden remains pending |
| 72-S6 | Format-preserving, admitted raw image capture and exposure capture | Implemented for canonical RGBA working formats; output contains native premultiplied storage bytes; this is not a display or PNG conversion |
| 72-S7 | Current packed plan v12 publishes precision at byte 152, numeric LUT encoding at 156 and five physical scratch format codes | Implemented; Kotlin production compiles and nine parser tests pass; earlier v11 receipts remain evidence for their own source stage |
| 72-S8 | Immutable two-key authored snapshots, 4081-point floating samples, dense frozen/editable storage, native admitted sample-only F32 tables, bounded scene source payload and editor key insertion | GPU/wire probes, resource/scene regressions and actual row/worker key-insertion tests pass; final end-to-end goldens pending |
| 72-S9 | Admitted working-to-SDR targets, clip/Reinhard and exposure policy, linear premultiplied interpolation, straight-alpha PNG and opaque linear-black MP4 bytes; required project precision and history; surface/plugin preview, frozen export and desktop display capture integration; project-wide settings dialog | Native output, project/history/surface probes, actual four-mode PNG/MP4 exports and worker UI/node captures pass; native menu/dialog, persistence, history and 60 UI captures pass; final full integration remains pending |
| 72-S10 | Final transparent-edge, gradient, HDR/glow, multiblend and mask goldens, full desktop/Host integration | Pending; the stage probes below do not close the issue |

UNORM16 is a normalized integer format, distinct from binary16 floating point.
The native path requires actual enabled normalized-format attachment support.
Float32 additionally requires enabled filtering/blending and applicable sample
support. A capability refusal is not counted as a successful pixel test.

## Source-bound local evidence

Each local receipt below contains the exact command, input SHA-256 values,
terminal exit status and logs under `artifacts/pr-queue/`. They describe separate
source stages; counts must not be added as if they were one full-suite run.

| Receipt directory | Result and evidence |
| --- | --- |
| `issue72-stage-contract-v1` | Nine portable contract tests |
| `issue72-stage-native-capabilities-v1` | Four capability tests at the foundation stage; the current five tests, including attachment-versus-MSAA support, also pass in the full regression below |
| `issue72-stage-native-formats-v1` | Four actual GPU format tests: numeric data/coverage, transfer/alpha isolation, HDR/fine Float32 and adjacent UNORM16 values |
| `issue72-stage-capture-layout-v1` | Raw row-stride and overflow layout test |
| `issue72-stage-native-capture-v1` | Five actual raw capture/admission tests, including foreign-owner refusal, rollback and device-failure priority |
| `issue72-stage-working-native-check-v2` | Current native test targets compile after replacing a private-helper access with the shared descriptor-capacity helper |
| `issue72-stage-working-pixels-v3` | Five tests: four-mode packed physical descriptors/capacity, real two-pass effect HDR/sign/detail and reuse, adjacent UNORM16 levels, floating temporal mean/resolve, vector/group MSAA and opacity in all four modes |
| `issue72-stage-composition-pixels-v1` | Five actual GPU tests: layer-input snapshots in both stack orders; float Multiply/Add/Screen and adjustment; half-coverage mask/projected matte/retained raster; budget refusal and recovery; ordinary nested texture pixels/capacity/reuse |
| `issue72-jvm-working-v1` | Production Kotlin compilation and six JVM parser tests; no Android device/GLES rendering validation |
| `issue72-stage-working-render-regression-v2` | Complete current `cargo test -p motion-render --offline -- --test-threads=1`: 601 passed, zero failed, one unchanged external-source probe ignored; includes the five working-precision and five composition-precision tests |
| `issue72-stage-semantic-meanings-v3` | Nine original portable tests plus four named-semantics tests: narrow storage, early precision refusal, strict metadata and same-precision padding |
| `issue72-stage-semantic-native-v1` | Five original raw-capture tests, four original GPU format/sampling tests and one new actual numeric capture test: weight above one, relative/linear depth, negative normal at zero fourth channel, odd row width and descriptor capacity |
| `issue72-stage-dense-model-v2` | Complete `motion-model` suite: 73 passed, zero failed/ignored; includes authored fine features, temporal sampling, immutable-source reuse, budget refusal, dense save/edit round trip and original malformed-input/cause tests |
| `issue72-stage-dense-core-v2` | Five original color-curve and nine original effect-editing tests pass, including freeze, seek order, atomic edits, dependencies and undo |
| `issue72-stage-dense-pixels-v2` | Four tests: real high-precision narrow features versus the bounded UNORM8 control; fine F32 gradients/temporal seeks with cached tables and pipelines; exact shared-allocation refusal/recovery; versioned packed native float bits and missing-source refusal before writing |
| `issue72-stage-dense-lifecycle-v1` | Actual high-precision two-to-one-to-two table lifecycle retains the surviving allocation, releases removed capacity and rebinds the replacement with correct pixels |
| `issue72-stage-dense-admission-v1` | Both unchanged curve-resource unit tests pass, including queued uploads/abandoned encoders and shrink/growth without retained capacity |
| `issue72-stage-dense-scene-core-v1` | Five original color-curve and nine effect-editing tests also pass after native Scene snapshot integration |
| `issue72-jvm-dense-v1` | Current production Kotlin compilation and nine parser tests pass; explicit encoding/precision, native float bytes, bounds and version refusals; no Android device/GLES rendering validation |
| `issue72-stage-dense-host-check-v1`, `issue72-stage-dense-native-check-v3` | Host compiles and all native render test targets check after wire/SDK integration; these checks are not pixel acceptance |
| `issue72-stage-output-native-check-v1` | All native test targets check with admitted explicit output conversion |
| `issue72-stage-output-pixels-v1` | Nine actual GPU tests: independent linear tone-map/exposure/alpha oracles, RGBA/BGRA with exactly one sRGB encoding, scaling before mapping, linear premultiplied transparent edges including encoded/straight input, real floating scene preview/PNG round trips and reuse, typed uniform admission/refusal, abandoned/submitted reservation retirement and foreign source/destination/encoder refusal even with a shared budget |
| `issue72-stage-output-regression-v1` | Fifteen unchanged raw-capture, preview-target budget and four-mode precision tests pass after the new output path; includes float raw PNG refusal, combined/peak capacity, retained-front recovery, native HDR/exposure and adjacent UNORM16 values |
| `issue72-stage-project-precision-v2` | Three model/core tests: four distinct required values in project format 26, strict missing/unknown/old-format refusal, save/reopen/undo/redo and failed-batch rollback, composition and frame-view inheritance, typed command catalogue schema |
| `issue72-stage-project-regression-v1` | Complete current model/core suites: 360 passed, zero failed, one unchanged manual large-document history performance measurement ignored; 79 test summaries; no assertion was weakened |
| `issue72-stage-switch-native-check-v1` | All native test targets check after extracting the existing planar pipelines into the same-device precision preparation module |
| `issue72-stage-switch-pixels-v2` | Three actual GPU tests: repeated four-mode changes retain the same device/queue/custom registry and equal fresh native pixels, including vector and numeric LUT paths; peak preparation admission and reduced-budget commit refusal preserve old mode/pixels and recover; narrow enabled devices, foreign owners sharing a budget and outdated preparations refuse before mutation |
| `issue72-stage-switch-regression-v1`, `issue72-stage-switch-host-check-v1` | All 24 current working/composition/curve/output pixel tests pass after the switch/cache fix; Host also compiles against project format 26 and the native transition API |
| `issue72-stage-output-integration-check-v1`, `issue72-stage-frozen-mp4-check-v1` | Host/media test targets check, including FFmpeg production export code |
| `issue72-stage-exposure-output-pixels-v3` | Nine original output tests and three new actual GPU tests: four-mode native/custom and exposure readback, retained identities/capacity, incomplete/foreign/stale-mode refusal, linear-black video output with opaque alpha and empty transparent input |
| `issue72-stage-host-png-pixels-v2` | Actual Host PNG in all four modes matches the straight-alpha SDR oracle and preserves project, history and view revision; four PNGs retained |
| `issue72-stage-frozen-png-pixels-v2` | Seven tests pass: all six original publication/resource/source/exposure tests and a new four-mode ordinary/exposure test after live edits and session close; eight PNGs retained |
| `issue72-stage-frozen-mp4-pixels-v3` | Eight actual frozen H.264/AAC encodes in four modes, ordinary/exposure, after live edits/session close; first/last RGB, alpha, duration and color metadata verified; eight MP4s and 16 decoded endpoint PNGs retained |
| `issue72-stage-frozen-mp4-pixels-v3/independent-ffmpeg` | Independent FFmpeg decodes all six Float32 exposure frames, maximum RGB error 3/255; ffprobe confirms 32x32 H.264 YUV420P, 30 fps, limited-range BT.709/sRGB and AAC |
| `issue72-stage-media-output-regression-v1` | Complete FFmpeg-enabled desktop media library: 43 passed, zero failed/ignored; includes original padding, linear alpha, source freezing, shared budgets, cancellation, temporal/nested video/audio and work-area tests |
| `issue72-stage-editor-integration-check-v1` | Host and desktop-media test targets compile after the editor target becomes an admitted native/SDR pair |
| `issue72-stage-editor-preview-pixels-v2` | All ten editor-recovery tests pass, including actual packaged-editor four-mode HDR/transparent PNG output, mode return to UNORM8, retained system/target identities, healthy capacity reuse, typed resize refusal, recovery and close retirement; four actual PNGs retained |
| `issue72-stage-editor-memory-regression-v1` | All 17 Host memory-policy tests pass, including retained native views across project replacement, runtime tracking, caller targets and shared session limits |
| `issue72-stage-desktop-display-check-v1` | FFmpeg-enabled desktop application and all its test targets compile, including the native-video study's explicit display readback |
| `issue72-stage-desktop-display-pixels-v1` | Real worker test passes in all four modes: UI pixel arrays and independent node-preview pixel arrays are identical across modes; session ledgers retain the four corresponding native formats; four UI and four node PNGs, default-Info log and report retained |
| `issue72-stage-desktop-node-capture-regression-v1` | Original installed-node admission/refusal test passes with unchanged assertions, including retained capture fingerprint, recovery and default file/Error stderr/stdout isolation |
| `issue72-stage-desktop-display-surface-regression-v1` | Original native surface replacement refusal test passes with unchanged assertions, preserving factory/front and default error channels |
| `issue72-stage-desktop-dense-key-v1` | All six row-navigation tests pass: three original selection/shortcut/default-error-log tests plus dense diamond insertion across four precisions, static/animated curves and shifted layer times, single-step undo/redo, serialization, actual bounded worker edits, locked-row and malformed-source refusal |
| `issue72-stage-desktop-dense-key-style-v1` | Existing `tools/ci_desktop.py::verify_style_consumers` passes; no palette/default stylesheet call or new token added |
| `issue72-stage-project-precision-ui-controls-v1` | Three native-control tests pass: four distinct modes in both locales; saved state and draft-only selection; popup input ownership; repeat, IME and focus-loss guards; missing/invalid metadata refusal |
| `issue72-stage-project-precision-ui-native-v1` | Actual desktop worker/window test passes across English/Chinese, 900x600/1600x1000 logical sizes and 1/1.5/2 renderer pixel ratios; real File menu route, Cancel, Apply, save/reopen metadata, actual Rgba32Float resource, single-step undo/redo; 60 actual GPU PNGs and report retained |
| `issue72-stage-project-precision-ui-style-v1`, `issue72-stage-project-precision-ui-check-v1` | Style-consumer guard and FFmpeg-enabled desktop application/all-test-target checks pass; these checks alone are not UI acceptance |

The failed `working-native-check-v1` receipt is retained. The first version of
the new vector-group test required one exact UNORM8 alpha byte; its `127` result
exposed an overstrict new oracle at the two rounding boundaries. Version v3 uses
the repository's existing one-LSB UNORM8 group contract. Its UNORM16 and floating
checks independently reject 8-bit steps. No pre-existing assertion was relaxed.
At that working-format stage, the sole changed pre-existing assertion tracked
the deliberately new packed-plan version in `projected_planes.rs`.

The first full render attempt (`issue72-stage-working-render-regression-v1`)
reported 221 passing unit tests and one failed snapshot-category admission test.
Its unchanged assertion caught an early subtraction of the old snapshot from
replacement admission. The implementation now checks the peak old-plus-new
capacity and deducts the old allocation only after successful replacement.
The failed receipt is retained. The focused unchanged snapshot test passes in
`issue72-stage-snapshot-peak-regression-v1`; the corrected full regression above
also passes. Its only ignored test is `optional_local_original_image_probe`,
which requires `MOTION_IMAGE_SOURCE` outside the repository; no test was newly
ignored to obtain this result.

The first applied named-semantics run (`semantic-meanings-v2`) exposed that
Serde's internally tagged unit variants accepted extra alpha metadata despite
the enum's `deny_unknown_fields`. Empty struct variants now reject these fields;
the JSON tags are unchanged. Existing coverage call sites were adapted to the
explicit struct syntax without weakening their assertions. The failing test and
the preliminary unapplied-patch/no-target attempt (`semantic-meanings-v1`) remain
recorded. Named capture tests check storage and raw values; they do not implement
a depth producer, a normal-space transform or a 3D lighting pipeline.

The preliminary dense model used 4096 uniformly spaced samples. The unchanged
`numeric_animation_and_curve_luts_are_independent_of_seek_order` assertion in
`dense-core-v1` caught floating interpolation drift after freezing. The current
4081-point nested grid subdivides each of the 255 preview intervals into 16 and
preserves all preview points exactly. The corrected current model and core
receipts above pass without changes to those assertions. Frozen/channel tables
accept the explicit 256- or 4081-point authored grid; this is not a project
migration or an attempt to invent detail from a low-resolution table.

Native dense tables are materialized only after GPU admission. Scene snapshots
bound selected authored payload to 32 MiB per scene before copying keys; cloned
contexts share Arcs. This payload bound excludes fixed containers/allocator
overhead and is separate from the shared GPU allocation budget. Dense tables
remain a finite approximation between taps. UNORM8 retains its 1024-byte table;
each high-precision table uses 65296 bytes of sample-only RGBA32Float storage.

The first native checks caught a helper-name/type mismatch and a binding-helper
fixture signature mismatch; their failed receipts are retained. The preliminary
new GPU budget probe incorrectly reduced `SourceTextures`; LUT allocations belong
to the existing `Effects` category under `RenderAllocations`. Version v2 injects
the shared allocation limit one byte below the exact dense-table requirement and
asserts the typed scope, required capacity, no retained LUT and successful recovery.
No pre-existing assertion was changed to obtain these results.

The explicit output converter retains its source and admitted 32-byte transform
uniform. Allocated encoders retain their reservations through discard or queue
completion. Input, destination and encoder ownership are checked separately from
shared-budget identity, before native binding or pass creation. Raw capture APIs
remain format-preserving; existing float PNG refusal is not removed. The new SDR
capture path emits straight-alpha, sRGB-encoded RGBA8 bytes after conversion at
working precision. This is an SDR policy, not HDR export or a color-management
system. No application UI setting or movie/Host export integration is claimed by
the native output probes.

Project format 26 requires `working_precision`; a newly created document starts
at `unorm8`, while loading a document with the field missing fails rather than
inventing a migration. The setting applies to the whole document, including
inactive composition/frame views. The first project-precision check caught one
missing frame-view initializer; the failed v1 receipt is retained. Core commands
persist and undo the setting. Application GPU preparation, surface preview and
settings controls were pending at that model-only stage; subsequent integration
and native UI evidence below now cover them.

The native precision-change API prepares new planar/vector/mask pipelines and
admitted resources before changing live fields. It performs no queue writes;
dropping a candidate releases its capacity immediately. An opt-in host constructor
enables supported floating features for future changes; existing constructors
retain their narrow feature requests. The first switch-pixel run caught a
same-size vector cache entry retaining an old F16 texture after selecting UNORM16.
Successful commit now retires working-format vector/mask/effect-input/nested
cache entries while preserving static sources and decoded video. The original
pixel-equality assertion passes in v2; the failed v1 receipt remains available.
Those native probes alone did not cover application preview-target conversion
or precision synchronization; subsequent Host/desktop and UI evidence below do.

The current changes preserve the
existing allocation leases, admission checks, rollback and submission ledger;
logical texture capacity includes each descriptor's actual format and samples.
This is not physical VRAM, bandwidth or a performance claim. No professional
grading, HDR input/output equivalence, or Android device validation is claimed.

The retained SDR preview wrapper has four additional GPU probes in
`issue72-stage-sdr-preview-pixels-v1`; all pass alongside the five unchanged raw
preview-budget tests. Each high-precision slot retains its native working texture,
admitted SDR texture and conversion binding; UNORM8 retains direct presentation.
Tests cover four-mode pixels/capacity/reuse, same-size precision and extent
replacement, preview-budget refusal without changing the displayed front, exact
32-byte uniform admission/refusal and recovery, required conversion before
publication, and foreign renderer/encoder refusal. The initial wrapper check
caught a new test using an obsolete readback-counter field name; its failed
receipt is retained. No pre-existing assertion changed. These probes verify the
wrapper itself. Application integration is separately tested below.

`issue72-stage-host-preview-check-v1` checks Host/media test targets.
`issue72-stage-host-preview-pixels-v2` exercises actual Windows surfaces for
ordinary and exposure frames: required initial F16 selection, all four modes,
native target formats, admitted native/SDR double-buffer capacity, unchanged
device/queue/surface, exact green-background/red-layer pixels, retained fronts,
pipeline-admission refusal without precision fallback, recovery and undo/redo.
The first version's new readback fixture incorrectly declared shader-visible
sRGB for an sRGB attachment, causing a format-validation failure. Correcting the
fixture to linear shader-visible values preserves every pixel assertion; the
failed receipt remains. No pre-existing assertion was weakened.

`issue72-stage-host-sampling-regression-v1` passes all five existing sampling
tests. `issue72-stage-host-surface-regression-v1` passes the original native
surface-replacement refusal test, and `issue72-stage-host-pressure-regression-v1`
passes the original ordinary/exposure pressure test. Their setup now explicitly
encodes output before committing the candidate, matching the new publication
contract. Existing assertions remain unchanged.

Project edits persist independently of GPU availability. Before content rendering,
the Host prepares and commits the requested mode on the owning device; failed
capability/admission returns its cause and retains the displayed SDR front. It
does not claim to have rendered the requested precision. The portable plan builder
also follows required project precision, including sessions without a surface.
Preview precision synchronization does not recreate the desktop painter, install
a fallback, change project history or alter layout/style.

The new export renderer starts at required project precision. PNG reads straight
sRGB bytes after native rendering/resolve; movies reuse an admitted native/SDR
target across frames and flatten onto linear black before output quantization.
The GPU video output writes alpha one, including empty transparent pixels, so
the existing encoder cannot multiply coverage again. Raw image APIs remain
format-preserving, and project precision is never reduced to obtain an export.

Failed output receipts are retained. Frozen PNG v1's new path assertion compared
Windows canonical/noncanonical spellings; v2 compares canonical identities. MP4
v1 exposed double black-matte application at the existing encoder boundary; the
opaque GPU output fixes it without changing the encoder or its original tests.
MP4 v2's 18x18 color fixture had isolated lossy padding-corner errors, reproduced
by independent FFmpeg. The color-policy fixture now uses macroblock-aligned
32x32 with the same per-channel tolerance of ten; existing nonaligned plane and
padding tests remain unchanged and pass in the full media regression. This
does not claim pixel-exact H.264 at arbitrary dimensions. No pre-existing test
assertion was changed in this export stage.

Actual output files and SHA-256 values are listed in
`artifacts/pr-queue/issue72-output-stage-manifest.json`. These constant-color
transparent/HDR probes do not replace the final comprehensive gradient/glow,
multilayer/mask, retained/nested temporal and product UI acceptance.

Plugin preview starts at project precision and prepares later mode changes on
the same renderer. Its retained target owns the native working image, a separate
SDR image, the converter and admitted staging. PNG bytes use straight alpha,
including the UNORM8 control. A failed replacement does not discard the old
target. Healthy repeated requests keep target identities and live capacity.
The real protocol's base64 PNG is compared with the independently checked GPU
pixels encoded through the production PNG encoder.

The editor-memory fixtures now use that genuine two-image target. Two existing
capacity expectations therefore increase from one RGBA8 image (`5*7*4`) to two
(`5*7*8`). This records additional ownership rather than relaxing a budget;
the retained-view retirement, exact peak, project and history assertions remain
unchanged. The failed new editor v1 test had incorrectly expected policy updates
to leave view revision unchanged. The existing policy deliberately invalidates
presentation once per update; v2 asserts those two exact invalidations and no
additional invalidation from refusal or recovery. No product behavior was
changed to satisfy that test.

Desktop UI and node screenshots allocate a semantic linear-premultiplied
UNORM8 image attachment, physically RGBA8 sRGB. This captures display pixels
without changing the project's working precision. The native-video study uses
the same display helper for its readback oracle. The independent N1 node draft
preview still renders internally at Float16; this stage verifies its display
capture, not Float32 draft arithmetic parity. Applied node effects use the
project's actual effect chain. No panel, layout, style, menu or interaction was
changed in this stage. These selected regressions do not replace the final
complete desktop/Host run or the separate project-setting UI acceptance below.

Curve-row key insertion now freezes the authored/interpolated 4081-point table
in every project mode; changing precision later therefore cannot reveal a prior
256-point editor freeze. The authored-data test uses a narrow feature that the
old preview grid entirely misses and verifies its exact interpolated peak.
The click still emits the existing two-command atomic batch, preserving layer
time conversion, selection, one-step history and failure observation. The frozen
table is authoritative, so obsolete per-channel overrides are removed from that
new value instead of duplicating their data. A genuine worker edit verifies the
serialized dense-table case stays within its unchanged 256 KiB request limit.

This changes authored key data, not row geometry or styling. The five §0.7
`effect_controls.rs` items (row stepping, boolean buttons, enum buttons, hover
alpha and hierarchy) are retained outside this callback-only change; their
current visual remediation status is pending verification. Other baseline
locations are untouched. No new dimension/color token, control, menu or
interaction gesture was introduced by that curve-key stage. The following UI
stage adds the separate project-wide settings entry. Comprehensive acceptance
remains pending.

## Project precision UI acceptance

File → Project Precision opens one project-wide draft using the existing native
dropdown and semantic popup/row/spacing/button tokens. It distinguishes integer
UNORM16 from Float16 and states that preview/PNG/video output is SDR. The menu
entry is disabled during observation. It reuses the existing core command and
error presentation; Apply is one history edit, while selection and Cancel do
not edit the project. No new dependency, asset, theme token or application
preference is introduced. DESIGN.md §4.1 records this new setting's contract.

The three control tests exercise all four values in both locales, popup versus
footer input ownership, Escape, focus loss, IME and repeated Enter. A separate
real-window test routes the actual menu action through Shell, selects Float32,
cancels a draft, then applies, saves, reloads stored project metadata and checks
undo/redo. It presents through the actual worker and confirms Rgba32Float in its
resource ledger. Screenshots do not edit project/history. Both locales run at
900x600 and 1600x1000 logical pixels with controlled 1/1.5/2 renderer pixel
ratios. This does not claim that the Windows OS DPI setting was changed.

`artifacts/pr-queue/issue72-project-precision-ui-manifest.json` binds all 60
original PNGs and the report by SHA-256/dimensions; three derived contact sheets
normalize them only for visual inspection. The File menu and option/saved-state
contact sheets were inspected: labels, enum field, popup and footer fit at every
combination. Default-Info file logs contain pre-handler edit starts and suppress
healthy Present starts. Existing environment font/memory-probe and external
Vulkan overlay discovery messages remain recorded; they were not suppressed or
treated as failures of the precision conversion.

For DESIGN.md §0.7, the path row-step, two-column, mixed-button, custom-number
and boolean-button entries are retained outside this new setting. The five
effect-control baseline entries remain retained outside this dialog/menu work,
as recorded above. Timeline columns and snap-guide color retain their documented
compatibility behavior. No baseline location is changed or claimed remediated;
their independent visual re-audit remains pending. Existing flat-menu dimensions
are also retained; the new row adds no literal dimension/color to panel code.

These four passing UI tests and selected checks do not replace the final full
render/Host/desktop run or the remaining gradient/glow, transparent-edge,
multiblend/mask and retained/nested temporal golden acceptance.

## Sources and provenance

All new shaders, probes and fixtures are independently authored repository code.
The white resource used by a test is generated from four constant bytes. No new
third-party asset, dependency or Adobe material is included. Format identifiers
and capabilities follow public primary references:

- [wgpu texture format flags](https://docs.rs/wgpu/latest/wgpu/struct.TextureFormatFeatureFlags.html)
- [Khronos EXT_texture_norm16, revision 6](https://registry.khronos.org/OpenGL/extensions/EXT/EXT_texture_norm16.txt)
- [Khronos EXT_color_buffer_float, revision 10](https://registry.khronos.org/OpenGL/extensions/EXT/EXT_color_buffer_float.txt)
- [Khronos EXT_float_blend, revision 2](https://registry.khronos.org/OpenGL/extensions/EXT/EXT_float_blend.txt)

These references describe behavior and enum identifiers; no specification text
or third-party implementation was copied into the product.
