# Issue 87 acceptance map

This map follows the current [issue #87](https://github.com/MAD-Producer/Kilonova/issues/87),
not the initial static importer boundary. It is a navigation and disposition
record, not an assertion that all acceptance gates have passed. Historical
checkpoint documents describe their own source revision; later checkpoints
supersede only the boundaries they explicitly implement.

The [composition boundary inventory](scene3d-composition-boundaries-current.md)
lists implemented pixel behavior, actual control locations and explicit limits
separately. It is not a claim of universal effect/nesting/alpha support.
The [native normative checkpoint](scene3d-normative-native-current.md) adds
independent source-based Box material/coordinate and SimpleSkin animation
comparisons. Its official alpha comparison remains outstanding.

The maintained scope is desktop. Android was explicitly excluded by the
requester after its retirement. Later scene nodes, ray tracing and volumes are
optional subsequent work, as stated in the issue. Background composition
prerendering (#82) is separate: its scheduler exists, but actual render/cache
integration and the advertised capability remain incomplete/false.

## Implementation and evidence

| Original requirement | Current evidence | Limit before final acceptance |
| --- | --- | --- |
| 87.1: static glTF/GLB mesh and owned buffers/textures | [Ingestion and native rendering](gltf-scene-current.md), [first-class project layers](project-model-depth-current.md) | Importing a file alone is not proof of rendering or project ownership |
| Paths, asset relinking, bounds, units and coordinates | Same scene/project ledgers; owned snapshot and asset validation controls | Final normative rendered-coordinate comparison remains open |
| 87.2: metallic-roughness PBR, normal/tangent, alpha modes, lights and instancing | Native pixel and budget controls in the scene ledger; shared geometry and [transparent planes](transparent-plane-depth-current.md) | External rendered-material comparison remains open; supported light/material profile is bounded |
| 87.3: morph/skin/animation and camera tracks | [Morph](gltf-morph-current.md), [skin](gltf-skin-current.md), [transform interpolation](gltf-transform-animation-current.md), [camera sampling](gltf-cameras-current.md), [camera activation](depth-camera-integrated-current.md) | Imported pose interpolation remains separate from authored Euler tracks; latest-main full regression is still needed |
| 87.4: calibrated AI depth and native camera depth | [Calibration](depth-calibration-current.md), [authoring](depth-calibration-authoring-current.md), [native depth surface](depth-surface-native-current.md), [project integration](depth-project-final-current.md) | Uncalibrated relative depth is explicitly refused rather than treated as metres |
| Mesh/plane occlusion and shared depth ownership | [Mixed planes](mixed-plane-depth-current.md), [textured planes](textured-plane-depth-current.md), [transparent planes](transparent-plane-depth-current.md), [image ownership](owned-depth-image-current.md) | General intersecting transparency is not order-independent transparency; see explicit native alpha policy |
| Masks and logical mattes retain source/material and physical depth semantics | [Masks](calibrated-depth-masks-current.md), [mattes](calibrated-depth-mattes-current.md), [materialization](calibrated-depth-materials-current.md), [spatial material](spatial-matte-materials-current.md), [shared consumers](shared-matte-inputs-current.md) | General external-input effects, nested/exposure configurations and unsupported source combinations retain explicit refusals; this is not universal effect support |
| Camera depth of field | [Native aperture checkpoint](camera-depth-of-field-current.md) | Four precisions, actual depth, animated focus and durable outputs passed the source-bound 1041-test related gate; blended/orthographic aperture remains explicitly unsupported; final workspace/platform gate remains separate |
| Saved/frozen reproducibility | Model/skin/morph ledgers plus [static frozen depth](frozen-depth-current.md), [temporal frozen depth](frozen-temporal-depth-current.md), [file-backed video export](video-depth-file-export-current.md) | Latest-main combined acceptance remains open |
| Budget admission and explicit capability refusal | Scene, shared-depth, deformation, calibrated material and frozen-output refusal/recovery controls | Local adapter results are not cross-platform or retired-device acceptance |

## Outstanding acceptance

1. The corrected aperture source passed 1041 distinct related tests, zero
   failures and zero unexecuted ignored fixtures. The one existing ignored
   manual history control was explicitly executed unchanged and counted once.
   Packed/legacy and nested aperture omission is guarded with actual refusal,
   unchanged caller buffers and zero-aperture recovery. Archive and publish
   this increment (committed as `023ef4b6`); final latest-main workspace/platform acceptance remains
   required. Native output proof does not establish portable optical support.
2. Complete the remaining official alpha comparison. Unmodified pinned Box
   rendering now matches an independent rough-metal, vertex-color and coordinate
   reference over 5,256 interior pixels in each of four precisions. Unmodified
   SimpleSkin matches raw-source LBS silhouettes at four times in each
   precision, including interpolated and negative rotations; frozen restore
   pixels match exactly. These are actual native comparisons, separate from
   the earlier ingestion-only checks. AlphaBlendModeTest is downloaded but
   has not passed an independent rendered reference.
3. Finish the advertised composition boundary inventory: distinguish actual
   supported nested/effect/exposure/transparent behavior from explicit refusal,
   and verify ordinary 2D/BSP results remain unchanged. A documented refusal is
   not an implementation of the refused operation.
4. Exercise sustained real-codec playback with measured duration and resource
   ownership. The accepted 150-frame/five-second beyond-cache export fixture
   proves durable temporal ownership and output matching, not long-duration
   playback throughput or trained-depth-model quality.
5. Fetch/rebase onto current main, run the complete maintained workspace and
   desktop checks on those imported sources, then require maintained-desktop
   cross-platform CI for the exact published head. Do not add overlapping
   checkpoint test counts into a cumulative coverage number.

No item above is closed by compilation alone, by a passing narrow control, or
by an earlier branch's green CI. This map keeps #87 open. #82's runtime is not
silently folded into a claim that the 3D route is complete.
