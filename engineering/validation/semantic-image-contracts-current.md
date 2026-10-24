# Semantic images and working precision: current delivery ledger

This is the active acceptance ledger for issue #72 on top of the merged #61
resource-admission and #67 physical scratch-lifetime implementations. Historical
experiments are not delivery evidence for the current source. The issue remains
open: the project setting, dense editor key insertion and product display/export
call sites are not yet connected to the native output conversion.

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
| 72-S8 | Immutable two-key authored snapshots, 4081-point floating samples, dense frozen/editable storage, native admitted sample-only F32 tables and bounded scene source payload | Actual GPU/wire probes and existing resource/scene regressions pass; editor key-insertion integration and final end-to-end goldens pending |
| 72-S9 | Native admitted working-to-SDR targets, explicit clip/Reinhard and exposure policy, linear premultiplied interpolation, straight-alpha PNG bytes | Nine actual GPU tests pass; persisted project setting and product preview/export call sites pending; no product UI changed in this stage |
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

The failed `working-native-check-v1` receipt is retained. The first version of
the new vector-group test required one exact UNORM8 alpha byte; its `127` result
exposed an overstrict new oracle at the two rounding boundaries. Version v3 uses
the repository's existing one-LSB UNORM8 group contract. Its UNORM16 and floating
checks independently reject 8-bit steps. No pre-existing assertion was relaxed.
The sole changed pre-existing assertion tracks the deliberately new packed-plan
version in `projected_planes.rs`.

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

The current changes preserve the
existing allocation leases, admission checks, rollback and submission ledger;
logical texture capacity includes each descriptor's actual format and samples.
This is not physical VRAM, bandwidth or a performance claim. No professional
grading, HDR input/output equivalence, or Android device validation is claimed.

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
