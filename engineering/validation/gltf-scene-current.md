# Issue #87 current acceptance

This is an incremental delivery ledger, not closure of #87. Work started after
checking the active branches/worktrees for glTF, mesh/PBR and issue #87 overlap.
The independent branch starts from fetched main
`3e19a47dc00aa17a013833e7ca117956c43ee43e`. Other agents' dirty worktrees were
not edited. No panels, UI behavior, Android code or old-project migrations were
changed.

## CPU ingestion and resource freeze

The new `motion-scene` library owns decoded triangle attributes, indices,
node/scene hierarchy, core material/sampler descriptors and source-identified
encoded resources. It validates external paths, container/accessor ranges,
types/strides, finite values, sparse indices, bounds, triangle topology, strict
forests, TRS/matrix semantics and finite payload budgets. Unsupported features
return explicit errors. A scene with only a mesh library is not auto-instanced.
See [API and precise limits](../host-api/scene-ingestion.md).

Frozen bundles retain the exact source plus owned external bytes in stable
locator order. Loading verifies version/digests/bounds and reimports privately,
without opening the original files. Source mismatch, changed resource bytes,
truncation, duplicate locators, unsupported bundle version and trailing bytes
are rejected. This is a library snapshot; it does not integrate with project
assets, desktop import or portable render-plan serialization yet.

Source-frozen local acceptance:

- 25 Rust tests passed, zero failed or ignored; formatting and strict Clippy
  passed for all targets of the new crate
- JSON glTF, GLB and data-URI triangles produce equal decoded geometry;
  interleaved normalized UVs, sparse zero-base data, linear RGB-to-RGBA color,
  MIME/slot/sampler checks and source-relative files are exercised
- Adversarial cases cover declared versus physical buffer lengths, overflow,
  alignment, NaN, reserved/out-of-range indices, GLB order/padding, disconnected
  cycles, multiple parents, wrong scene roots, shear and nonunit quaternions
- Explicit metre-to-pixel scale preserves authored negative scale and singular
  TRS; matrices never pass through existing layer Euler tracks
- A controlled blocking resolver verifies the Info start exists before resolver
  completion; its original IO kind and request/source identity survive failure
- Real-directory replacement leaves an already imported scene and its digest
  unchanged; freezing/reimporting is independent of the original resolver

External Khronos models are pinned at
`8f492390ddc7a2ae221a1369ef560589cf8c70fb`: Avocado (Microsoft, CC0-1.0) and
BoxVertexColors (Marco Hutter, CC0-1.0). All 15 downloaded files were hashed
against the fixture manifest before running the actual Rust binary. Both JSON
and GLB variants passed for each model. Avocado has 406 vertices, 682 triangles
and three encoded images; the box has 24 vertices and 12 triangles. Decoded
geometry, materials, textures, samplers, image identities and instance matrices
match across packaging variants; each generated frozen bundle reimports to an
equal scene. This compares ingestion, not a rendered reference image.

The actual desktop logger is compiled unchanged into an external CPU probe.
With `MOTION_LOG` absent, all four positive probes contain Info starts in file
logs and empty stdout/stderr. A real traversal refusal contains the request,
source, phase, elapsed time and original cause in both the file and stderr;
Info remains file-only and stdout remains empty. The probe has no Engine and
does not fabricate engine flight state. Import does not yet run through a
desktop command.

Evidence: [source, controllers, raw outputs, official provenance and receipts](data/gltf-ingestion-3e19a47d.zip),
SHA-256 `5b6457e7d56bc1cd4a93c10ac1d546815b016954a2e7eaf7b8016db0ad9b6d07`,
214,937 bytes, 65 entries; each entry was read back and SHA-256 checked. Original
models, textures, binaries and generated model bundles are excluded from the
archive and product. Source/runtime hashes bind both accepted probes and tests;
the numerical reports are credited derivatives used only as technical evidence.

## OBJ/MTL addition

The user explicitly added OBJ to the same scene work. A fresh branch/worktree
overlap check found no active OBJ/Wavefront implementation to duplicate. This
addition remains on the owned #87 branch and draft PR #516, after the glTF
foundation checkpoint `af959f4e1f50d264c6f58074bfcc784f8dc72f2a`.

Accepted: a bounded static polygon importer with concave triangulation,
positive/negative separate attribute indices, explicit coordinate/unit/UV
options, object/group metadata and smoothing across UV seams. Classic MTL
values and map options remain `MaterialModel::Wavefront`; there is no implicit
Phong-to-PBR conversion. Owned raw MTL and image resources are included in a
distinct OBJ snapshot, with integrity-checked import options. Unsupported
directives, missing materials/resources and invalid geometry return explicit
causes. See the [supported profile and exclusions](../host-api/scene-ingestion.md#static-wavefront-obj-and-mtl).

Current source-frozen acceptance supersedes the earlier crate-only test count:

- 45 Rust tests passed, zero failed/ignored: the original 25 glTF/freeze tests
  remain unchanged, with 20 added OBJ tests; formatting and strict all-target
  Clippy passed
- Controlled blocking MTL resolution verifies an Info start is published before
  resolver completion, and the original IO cause/request/source survive failure
- A real-directory OBJ+MTL+PNG import matches an independently authored glTF
  triangle's decoded geometry, normalized normals, UVs, instances and image
  identity; classic material/map metadata is checked separately
- A second process restores an equal scene after all five original comparison
  input files have been removed; deterministic freezing and digest/length/
  option-corruption refusal are also exercised
- Eight actual-binary cases cover the two format imports, source-free restore,
  traversal, missing MTL, bad corner index, unsupported script directive and
  damaged frozen options; default Info starts are file-visible, five errors
  reach both file and stderr, and stdout is empty throughout
- All four pinned Khronos model variants were rerun against the changed runtime,
  with the same geometry/material/texture/instance and freeze checks; these
  remain CPU ingestion comparisons, rather than rendered-image acceptance

The first external OBJ probe was rejected because its new controller compared
an f32 value serialized through JSON with a binary64 literal `0.2`. The second
uses the exact IEEE f32 golden. No product code, existing assertion, error
bound or acceptance criterion was changed for that correction; the first
controller and incremental receipt are retained as superseded evidence.

Evidence: [OBJ source, controllers, raw outputs, current glTF probes and receipts](data/obj-ingestion-af959f4e-v2.zip),
SHA-256 `b56d16ec1c7c28bed7774ab1a622cdb3ddf9e3d6b74e93053bca9c4b2c0c125b`,
267,995 bytes, 108 entries, all read back and SHA-256 verified. Original models,
images, binary probes and encoded model bundles are excluded. The independent
OBJ/MTL/PNG/glTF fixture generator and individual identities are included.
Source/runtime hashes were unchanged across each accepted run. The initial
glTF archive above remains a historical checkpoint, not current-runtime proof.
The final OBJ rerun also includes explicit refusal of overflowing sRGB emissive
conversion; a finite authored number cannot introduce infinity into the scene.

Not delivered by this addition: desktop import/project asset registration,
GPU texture decode/material rendering, PBR mapping policy, freeform geometry,
reflection/spectral MTL, external PBR extensions, animated deformation or mixed
depth. OBJ parsing is not complete #87 acceptance. No UI/panel/Android change
or old-project compatibility layer is added.

## Remaining issue acceptance

| Item | Current disposition |
| --- | --- |
| #87.1 static geometry and owned resources | CPU foundation accepted above; host asset registration/relink transactions and desktop/project integration still pending |
| #87.2 core PBR, normal/tangent, alpha, lights, instancing | Descriptors retained; actual GPU texture decoding, material shading and budget/capability gates pending |
| #87.3 skin, morph, animation and cameras | Explicitly unsupported by this static profile; implementation and quaternion/cubic sampling proof pending |
| #87.4 unified camera/AI depth, occlusion, DOF | Pending explicit projection/calibration and mixed 2D/3D, parent/matte/BSP regressions |
| #87.5 scene nodes, optional RT/volume and export disposition | Later work; no unsupported feature may be silently stripped from an export |
| final native and cross-platform acceptance | Not established by this crate's local gate; required before completing #87 |

Only maintained desktop targets are in delivery scope. Android is retired and
is not implemented or validated. Static import success is not full scene,
PBR, animation, depth-compositing or real-time performance support.
