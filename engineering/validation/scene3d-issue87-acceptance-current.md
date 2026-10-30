# Issue 87 acceptance map

This map follows the current [issue #87](https://github.com/MAD-Producer/Kilonova/issues/87),
not the initial static importer boundary. It is a navigation and disposition
record, not an assertion that all acceptance gates have passed. Historical
checkpoint documents describe their own source revision; later checkpoints
supersede only the boundaries they explicitly implement.

The [composition boundary inventory](scene3d-composition-boundaries-current.md)
lists implemented pixel behavior, actual control locations and explicit limits
separately. It is not a claim of universal effect/nesting/alpha support.
The [original-scope disposition](scene3d-composition-disposition-current.md)
binds nineteen existing controls to actual execution and reconciles this
inventory with the original mixed-composition/BSP acceptance; it adds no new
tests to the counts.
The [native normative checkpoint](scene3d-normative-native-current.md) adds
independent source-based Box material/coordinate and SimpleSkin animation
comparisons. The [independent alpha checkpoint](scene3d-normative-alpha-native-current.md)
adds the whole unmodified AlphaBlendModeTest scene, source-derived filtering,
cutoff and blending references, four precisions and exact frozen restoration.
The [sustained native playback checkpoint](scene3d-long-native-playback-current.md)
records a measured 60-second real-codec host run, bounded retention and actual
owner retirement. Its small controlled fixture is not a high-resolution or
trained-model benchmark.

The maintained scope is desktop. Android was explicitly excluded by the
requester after its retirement. Later scene nodes, ray tracing and volumes are
optional subsequent work, as stated in the issue. Background composition
prerendering (#82) is separate: its scheduler exists, but actual render/cache
integration and the advertised capability remain incomplete/false.

## Implementation and evidence

| Original requirement | Current evidence | Limit before final acceptance |
| --- | --- | --- |
| 87.1: static glTF/GLB mesh and owned buffers/textures | [Ingestion and native rendering](gltf-scene-current.md), [first-class project layers](project-model-depth-current.md) | Importing a file alone is not proof of rendering or project ownership |
| Paths, asset relinking, bounds, units and coordinates | Same scene/project ledgers; owned snapshot and asset validation controls; independent native Box coordinate comparison; [desktop import/library checkpoint](scene3d-model-library-current.md) | Controlled native routes are accepted locally; physical OS input and automatic bounds fit are not claimed |
| 87.2: metallic-roughness PBR, normal/tangent, alpha modes, lights and instancing | Native pixel and budget controls in the scene ledger; shared geometry and [transparent planes](transparent-plane-depth-current.md); independent Box and AlphaBlendModeTest references | Supported light/material profile is bounded; alpha comparison does not prove general intersecting transparency |
| 87.3: morph/skin/animation and camera tracks | [Morph](gltf-morph-current.md), [skin](gltf-skin-current.md), [transform interpolation](gltf-transform-animation-current.md), [camera sampling](gltf-cameras-current.md), [camera activation](depth-camera-integrated-current.md); [maintained workspace](scene3d-maintained-workspace-current.md) | Imported pose interpolation remains separate from authored Euler tracks; later source needs its own published-head CI |
| 87.4: calibrated AI depth and native camera depth | [Calibration](depth-calibration-current.md), [authoring](depth-calibration-authoring-current.md), [native depth surface](depth-surface-native-current.md), [project integration](depth-project-final-current.md) | Uncalibrated relative depth is explicitly refused rather than treated as metres |
| Mesh/plane occlusion and shared depth ownership | [Mixed planes](mixed-plane-depth-current.md), [textured planes](textured-plane-depth-current.md), [transparent planes](transparent-plane-depth-current.md), [image ownership](owned-depth-image-current.md) | General intersecting transparency is not order-independent transparency; see explicit native alpha policy |
| Masks and logical mattes retain source/material and physical depth semantics | [Masks](calibrated-depth-masks-current.md), [mattes](calibrated-depth-mattes-current.md), [materialization](calibrated-depth-materials-current.md), [spatial material](spatial-matte-materials-current.md), [shared consumers](shared-matte-inputs-current.md) | General external-input effects, nested/exposure configurations and unsupported source combinations retain explicit refusals; this is not universal effect support |
| Camera depth of field | [Native aperture checkpoint](camera-depth-of-field-current.md) | Four precisions, actual depth, animated focus and durable outputs passed the source-bound 1041-test related gate; blended/orthographic aperture remains explicitly unsupported; final workspace/platform gate remains separate |
| Saved/frozen reproducibility | Model/skin/morph ledgers plus [static frozen depth](frozen-depth-current.md), [temporal frozen depth](frozen-temporal-depth-current.md), [file-backed video export](video-depth-file-export-current.md) | Latest-main combined acceptance remains open |
| Budget admission and explicit capability refusal | Scene, shared-depth, deformation, calibrated material and frozen-output refusal/recovery controls | Local adapter results are not cross-platform or retired-device acceptance |

## Acceptance status and remaining gate

1. The corrected aperture source passed 1041 distinct related tests, zero
   failures and zero unexecuted ignored fixtures. The one existing ignored
   manual history control was explicitly executed unchanged and counted once.
   Packed/legacy and nested aperture omission is guarded with actual refusal,
   unchanged caller buffers and zero-aperture recovery. The increment was
   merged through [PR 558](https://github.com/MAD-Producer/Kilonova/pull/558)
   as `ed1a84a4` after all five exact-head required checks passed. Its actual
   squash title and body are Chinese. The merge tree is byte-identical to
   source head `594e810d`. Its separate maintained-workspace gate passed,
   including the three explicitly executed manual controls; all four main
   release platforms passed. [Exact counts and limits](scene3d-maintained-workspace-current.md)
   remain source-bound. Native output proof does not establish portable optical support.
2. Official sample comparisons passed their own source-bound gates. Unmodified pinned Box
   rendering now matches an independent rough-metal, vertex-color and coordinate
   reference over 5,256 interior pixels in each of four precisions. Unmodified
   SimpleSkin matches raw-source LBS silhouettes at four times in each
   precision, including interpolated and negative rotations; frozen restore
   pixels match exactly. These are actual native comparisons, separate from
   the earlier ingestion-only checks. Whole unmodified AlphaBlendModeTest
   matches 2,024 independently selected interior pixels per precision, with
   wrong-opaque and wrong-cutoff negative controls, default-budget refusal and
   exact frozen restoration. These separate checkpoints do not replace the
   latest-main workspace and platform gates.
3. The original mixed-composition inventory is reconciled against nineteen
   actually executed existing controls and exact source hashes. Mixed
   depth/parent/camera/matte cases and ordinary BSP behavior are demonstrated
   within the documented profile. Arbitrary nested/effect/exposure/transparent
   combinations retain their explicit limitations; they are not implementations
   of the refused operations. The full local workspace remains separate.
4. The sustained native host checkpoint passed 60.010626 seconds with 1,762
   newly presented frames, 1,775 stored depth samples, 128 live samples and zero
   logical admission after actual owner retirement. Its 62 checkpoints verify
   original decoded PTS/fingerprint and physical occlusion/color. This replaces
   the earlier short-export-only playback evidence. It proves this bounded
   32x32 controlled-backend run, not trained-model quality or high-resolution
   throughput; keep those limits visible in the final acceptance.
5. Main was fetched after the PR 558 merge and its complete Git tree was
   checked identical to the accepted full-workspace source. The distinct
   maintained total is 3,782 passed, zero failed, with 27 other manual controls
   excluded and enumerated. The resulting main release run passed all eight
   jobs, including all four platforms. Import/library work is aligned with
   that main baseline and passed its own local affected regression. Its
   published-head cross-platform CI remains pending; the older green run
   cannot substitute for that gate. Do not add overlapping checkpoint counts.
6. Close the actual desktop model-import gap. Existing scene/project loading
   accepts owned models, but the normal desktop file route previously treated
   model files as audio. A new worker operation connects composition file
   drops and revision-checked MCP import to bounded glTF/GLB/OBJ ingestion,
   owned frozen dependencies and one atomic undo boundary. Import/reopen,
   independent undo/redo, actual failures, default file/Error stderr and
   JSON-RPC-only stdout passed their source-bound controls. Project-library
   registration, repeated placement, picker filters, actual context-menu and
   pointer-release drag routes are now connected. The current library source
   passed 1,392 desktop tests, zero failures and a separately executed native
   control with four locale/size cases. The 26 ignored rows are enumerated,
   not passes. [Verified evidence](scene3d-model-library-current.md) retains
   the initial 900x600 layout failure: the default Project viewport is still
   unavailable there; the narrow native controls use supported tab docking.
   No product layout or hit geometry was changed to conceal that limitation.

No item above is closed by compilation alone, by a passing narrow control, or
by an earlier branch's green CI. This map keeps #87 open. #82's runtime is not
silently folded into a claim that the 3D route is complete.
