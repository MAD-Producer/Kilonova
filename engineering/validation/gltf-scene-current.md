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
