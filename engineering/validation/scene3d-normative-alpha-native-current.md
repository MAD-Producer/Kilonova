# Independent native alpha sample checkpoint

Issue 87's normative transparency comparison now has actual native pixels
from the unmodified Khronos AlphaBlendModeTest. The product source matches
PR 558 head `594e810df818bdafe34f0b9c08455828168b634a` and the accepted 1041-test aperture gate.
This checkpoint does not by itself close the issue or prove universal OIT.

## Source and licensing

The sample is pinned to glTF Sample Assets revision
`8f492390ddc7a2ae221a1369ef560589cf8c70fb`, by Ed Mackey / Analytical Graphics
Inc., 2018, CC-BY-4.0. Author, exact source, license and modification scope
are recorded in `ATTRIBUTION.md`; the external manifest retains every original
URL, byte length and SHA-256. Original glTF, buffers and textures are unchanged
and remain external. This archive contains independently authored probes,
reference code, native validation readbacks and reports; no upstream original
assets or screenshots are bundled in the product.

## Independent reference

The [glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#alpha-coverage)
defines OPAQUE, MASK and source-over BLEND, with linear alpha and sRGB base
color. The probe imports the whole original scene (nine draws, two blended
draws), including the occluding textured bed and arrows. An orthographic
camera and constant diffuse irradiance with no direct lights isolate these
rules from direct-light/normal-map differences. This is not a beauty-render
or environment-lighting reference.

Python independently decodes raw buffer accessors and original node
transforms, intersects source triangles at output pixel centres, interpolates
UVs, decodes original textures with Pillow, integrates linear area mip levels
with float64 NumPy, samples bilinear/trilinear repeat textures and applies
alpha cutoff and source-over. It does not consume converted importer geometry,
read back shader inputs or replay the native WGSL program. The reference and
negative-control pixel selection were persisted and hashed before native
execution, and their hash was verified unchanged after comparison.

There are 2024 interior pixels per precision: OPAQUE 415, BLEND 401,
cutoff 0.25 400, default cutoff 0.5 404, cutoff 0.75 404. Source-derived
triangle ties and uncertain cutoff/filter boundary pixels are excluded before
the GPU run. A fixed +/-0.02 mip LOD envelope and 0.001 sample rounding bound
account for hardware filtering; fixed final tolerances allow independent JPEG
decoder and output quantization differences. The controls distinguish an
all-opaque implementation at 953 pixels and a forced-default-cutoff
implementation at 158 pixels. Neither tolerances nor coverage assertions were
relaxed after seeing native values.

| Precision | Checked pixels | Maximum error outside reference bounds | Allowed error |
| --- | --- | --- | --- |
| Unorm8 | 2024 | 0.0081877549 | 0.016 |
| Unorm16 | 2024 | 0.00280658562 | 0.012 |
| Float16 | 2024 | 0.0032898004 | 0.012 |
| Float32 | 2024 | 0.00280157876 | 0.012 |

Each precision also checks a transparent clear corner and exact byte equality
after freezing, dropping the original GPU scene owner and restoring its owned
assets. All comparisons passed. This proves this source/camera/light/material
profile, not arbitrary intersecting translucent surfaces, which retain the
documented sorted-alpha limitation.

## Budget and output observations

The original 2048-square textures require 192 MiB conservative transient
decode admission. Default 128 MiB refusal is asserted in each precision, then
an explicit 256 MiB scene allowance is used for the reference run. No product
default, asset size or texture content was changed. Default-level Info start
is present, all four actual budget Errors reach both file and stderr, and
native stdout remains empty. The existing missing Epic overlay manifest
messages are retained in raw logs; they did not prevent successful rendering.

## Retained failures and evidence

The first CPU reference used unweighted neighborhood extrema and failed its
minimum stable-pixel coverage assertion before native execution. Version 2
replaced that conservative approximation with independently filtered source
samples while preserving the coverage/tolerance assertions. The first native
attempt then correctly refused the default decode budget; its original probe,
panic stack and logs are retained. The final probe explicitly tests that
refusal before using the larger admission. These are separate setup/reference
failures, not silently omitted product regressions.

Archive: `data/scene3d-normative-alpha-native-v1.zip`, 48 entries, 6141841
bytes, SHA-256 `23a6ecf127420d40a7029060c3869e775a5ec679fff0a24a420198ff21e1c632`. CRC and every member were read back and compared with
the original files. Full-workspace and exact-head maintained-desktop platform
acceptance remain separate gates. No UI, panels, product dependencies or
rendering source changed for this checkpoint.
