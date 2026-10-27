# Owned scene ingestion

`motion-scene` is the CPU ingestion foundation for issue #87. It is a workspace
library without GPU, window, inference, filesystem-at-render-time or Android
dependencies. The native consumer below and the separate project `ModelClip`
adapter use its owned data. Successful parsing alone does not establish PBR or
mixed 3D composition support. Current project and native acceptance is recorded
in the [model layer](../validation/model-layer-current.md) and
[camera](../validation/gltf-cameras-current.md) ledgers.

The independent implementation follows the [Khronos glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html),
sections 3.4–3.9 and 4, revision `8e798b02d254cea97659a333cfcb20875b62bdd4`.

## Core import profile

`import(context, source, resolver, limits)` accepts JSON glTF 2.0 and GLB version
2. It validates container ranges/order/alignment, resource lengths, referenced
views, attribute types/strides, sparse substitutions, finite values, position
bounds, triangle indices and node forests. Supported primitive topology is a
triangle list. Positions, optional normals/tangents, two UV sets and one linear
vertex color set are decoded into owned arrays. Missing normals stay absent:
the future renderer must generate flat normals and ignore authored tangents in
that case. Import never invents smooth normals.

Core metallic-roughness factors, texture slots, normal scale, occlusion strength,
emissive factors, alpha mode/cutoff, double-sided state and sampler constants
are retained. PNG/JPEG bytes and their MIME signatures are retained without
decoding. This is not image validity, pixel-budget, texture upload or PBR
acceptance. Color/emissive texture transfers are sRGB; the other texture slots
and vertex multipliers are linear. No material shader is claimed here.

Skins, morphs, nontriangle topology, unsupported attribute semantics and required
extensions produce explicit `Unsupported` errors. Core transform clips and
cameras follow the supported profiles below. An
optional extension uses its core fallback and appears in the scene's bounded
`ignored_optional_extensions` list and one import warning. It must not be
presented as full-fidelity extension support. A library without scenes remains
a library, rather than implicitly instancing every mesh.

## Resources and ownership

`AssetResolver` receives decoded, validated local relative paths and a remaining
byte allowance. Network schemes, absolute paths, parent traversal, backslashes,
queries, fragments and control characters are refused before resolver entry.
The resolver must honor the allowance before allocation. Data URI base64 decode
has a checked output bound. PNG/JPEG data URIs and the two core buffer MIME
types are supported.

`DirectoryResolver` canonicalizes the explicit root and resource, checks
confinement, file type and size, and bounds reads from the opened handle. This
assumes no adversarial concurrent rename of the import directory: it is not an
OS filesystem sandbox. A caller needing that isolation should resolve from an
already-owned resource map. A missing resource returns the original IO cause.
All source-dependent bytes are held by the imported scene, with SHA-256 and an
optional decoded relative locator for explicit relinking. Rendering cannot
reopen them. Replacing a file after import does not change an existing scene.

`scene.freeze(original_source, max_bundle_bytes)` writes the exact source and
deduplicated external resources in deterministic locator order. The source
digest must match the import, and a locator that resolved to different contents
during one import cannot be frozen ambiguously. `import_frozen` checks version,
lengths, digests, duplicate locators and trailing data, then reruns the same
importer from a private resource map. Neither normalized runtime arrays nor
serialized claims are trusted. It requires no original files or network. Only
the current bundle version is accepted; there is no old-project migration.

## Coordinates and budgets

Mesh arrays and authored column-major node matrices remain in glTF's
right-handed, +Y-up metre coordinates. TRS composes as T × R × S; quaternion
XYZW never passes through the existing layer Euler tracks. Sheared, non-affine,
mixed matrix/TRS and invalid quaternion inputs are rejected. Singular authored
TRS remains representable. Instancing preserves negative-scale winding rather
than reversing indices implicitly.

`scene.instances(scene_index, pixels_per_metre)` requires explicit finite,
positive scale and computes world matrices iteratively. The existing host world
space also has +Y up. The project model adapter applies camera/layer
placement explicitly; import does not select a screen origin or silently fit a
model to the viewport. Transform overflow returns an error.

Default limits are 64 MiB source, 16 MiB JSON, 64 MiB per resource, 256 MiB
retained resource/decoded attribute payload, 16,384 descriptors per category,
one million decoded vertices, three million indices and 128 node levels.
Repeated primitives consume the aggregate decoded budgets even when they read
one accessor. These are payload/structure limits, not a process-memory cap:
JSON structures, temporary sparse indices, allocator capacity and caller-owned
source/bundle bytes also occupy memory. No GPU budget or mobile capability
claim follows from them. The caller separately bounds encoded frozen bytes.

Info start records precede parsing, digest checks and potentially blocking
resolver entry. Import errors include request/source IDs, phase/object, elapsed
time and the original typed cause. Log text is bounded; default file/stderr
routing belongs to the existing desktop logger. Done records use Debug. The
caller must schedule import off the paint thread and publish its worker's
in-flight state before calling this synchronous API. No worker restart, retry
or command timeout is introduced.

## Static Wavefront OBJ and MTL

`obj::import_obj(context, source, resolver, options, limits)` uses the same
owned scene/resource API. The independent implementation references the Library
of Congress descriptions of [OBJ](https://www.loc.gov/preservation/digital/formats/fdd/fdd000507.shtml)
(2025-02-18) and [MTL](https://www.loc.gov/preservation/digital/formats/fdd/fdd000508.shtml)
(2019-10-04). This is a polygon profile, not the entire historical format.

Supported records are `v`, `vt`, `vn`, `f`, `o`, `g`, `s`, `mtllib` and `usemtl`.
Face corners support all four position/UV/normal forms, positive one-based and
negative relative indices. Attributes must already be defined. A face must use
one consistent attribute layout. XYZ, nonzero homogeneous XYZW and the explicit
XYZ+linear-RGB vertex extension are accepted; ambiguous seven-component vertex
records are rejected. UVs have one or two components, or a third zero component.
UTF-8 text supports BOM, LF/CRLF, comments, quoted names and line continuation.

Dominant-axis ear clipping triangulates simple concave polygons while preserving
front faces. Self-intersections, cancelling/degenerate faces and polygons over
256 corners fail explicitly. A nonplanar polygon retains its original 3D
positions; the metadata counts projected faces and the importer emits one Warn
per import with a nonzero count. This is a triangulation policy, not a claim
that nonplanar faces have a uniquely defined surface. A shared 16-million-step
work bound limits polygon intersection and ear searches.

Authored normals are normalized and retained. If missing, an enabled smoothing
group generates area-weighted normals across faces of the same object and
smoothing group, including UV seams and material splits. Named `g` membership
does not create a hard edge. Smoothing off leaves normals absent for later flat
shading. Opposing contributions that cancel to zero fail explicitly. Object,
named groups, smoothing group and source line survive in `ObjMetadata`.

OBJ has no mandatory unit or coordinate convention. `ObjOptions` explicitly
selects positive `metres_per_unit`, Y/Z up, right/left handedness, V flipping and
MTL color interpretation. Defaults are one metre/unit, right-handed Y up,
V flip enabled and linear MTL RGB. These defaults are application choices.
Geometry is converted to canonical right-handed Y-up metres; reflected input
also reverses winding and transforms normals consistently. Options survive
freezing and are integrity checked. Instancing still requires a separate
positive pixel/metre scale. No auto-fit or placement heuristic is introduced.

Classic MTL is retained as `MaterialModel::Wavefront`, including RGB
`Ka/Kd/Ks/Ke/Tf`, `Ns`, `Ni`, `d`/the common inverse `Tr` convention and `illum`.
The diffuse, ambient, specular, emissive, opacity, exponent, bump, normal and
displacement maps retain their image identity and authored `-o`, `-s`,
`-clamp`, `-bm` and `-imfchan` options. `Ke` and `norm` are supported exporter
conventions. Bump remains a height descriptor, rather than pretending to be a
normal map. Diffuse/alpha/emissive scalar mirrors are diagnostic conveniences;
generic PBR texture slots remain unset. No guessed Phong-to-PBR conversion or
material rendering is claimed. Duplicate material names/map kinds, undefined
materials, contradictory dissolve values and unknown properties/options fail.
Spectral/XYZ colors, reflection maps and nonstandard PBR extensions are outside
this profile. Map filenames that are solely numeric are outside the option
parser's supported profile.

OBJ filenames are literal local names, with no URI percent decoding. Quoted
spaces and backslash separators are accepted. A texture is resolved relative
to its containing MTL; normalization may remove `..` only while staying inside
the explicit OBJ import root. Absolute/network paths and root escape fail
before reading. Missing MTL/texture files preserve their original IO cause.
PNG/JPEG encoded images are deduplicated by normalized path, with a sampler per
map. The same signature-only image limitation as glTF applies. Raw MTL bytes
are retained as extra resources. No script, shell, `call`, freeform, line or
point directive is executed or silently discarded.

OBJ reuses source/resource/decoded payload and descriptor limits. Material text
also respects the JSON-text byte limit. Logical records are at most 64 KiB and
1,025 tokens; names are at most 1,024 bytes and group membership at most 32.
Raw attribute arrays are individually bounded before decoding; final duplicate
vertices and indices are charged again against aggregate limits. These bounds
exclude parser containers, names, temporary storage and allocator overhead:
they are not a process RAM cap. Frozen OBJ uses a distinct current envelope
with hashed import options, exact source and owned MTL/images. glTF's current
envelope stays byte-for-byte unchanged. Reimport requires no original files.

This adds a CPU API only. Desktop import commands, project asset transactions,
GPU shading, PBR mapping policy, texture decoding, mixed depth and interactive
model manipulation remain subsequent #87 work shared with glTF.


## Native static scene consumer

`motion_render::scene_gpu::GpuScene::upload` consumes an immutable owned
`motion_scene::Scene`. The caller supplies numeric request/source IDs and
explicit decode, resident, image-edge and draw limits. This is a backend API;
it does not by itself register a desktop asset or install a model layer.

An upload emits Info start before image decode or device entry. Error records
retain request, source, phase, elapsed time and the bounded original cause;
image codec errors also identify the image index. The host serial worker must
publish its own in-flight state before entering this API. This consumer does
not create a new worker, timeout or restart policy.

- Resources use the existing renderer allocation owner, global admission and
  upload/encoder/submission retirement. A scene cannot move to another renderer
- `SceneGpuLimits::resident_bytes` measures logical mesh, material-buffer and
  image-mip capacities. Frame uniforms and color/depth targets also join the
  renderer's global allowance; neither measure is process RAM or driver VRAM
- Decode admission estimates 48 bytes per pixel before allocating decoded
  storage, covering concurrent conversion/mip/upload pixel containers. Encoded
  scene resources and codec/allocator/driver overhead are outside that estimate
- PNG/JPEG images produce complete mip chains with linear-light area integration,
  including odd final rows and columns. Color images decode sRGB; normal,
  metallic/roughness and occlusion maps retain linear numeric channels
- PNG16 uses filterable Rgba32Float storage to retain precision after color
  conversion. The device must have FLOAT32_FILTERABLE enabled; refusal is
  explicit, with no silent byte or half-float conversion

`SceneFrame` chooses the imported scene and pixels-per-metre scale, a finite
invertible camera projection, a finite affine world transform, up to four
point/directional lights, linear ambient/background values and explicit 1x/4x
sampling. The camera and world share right-handed Y-up coordinates. Point-light
attenuation uses distance in metres. Frame-local immutable uniforms allow
multiple encodes before one submission without overwriting the first draw.

The core glTF path uses metallic-roughness shading with GGX/Smith/Schlick,
normal/tangent inputs, selected UV0/UV1, vertex color, occlusion, emissive and
opaque/mask/blend alpha. The ambient term is a constant irradiance approximation,
not environment-map lighting. A finite raster sample cannot represent a perfect
zero-roughness delta lobe; the slope has an explicit numerical floor of 1e-4.

OBJ remains a separate classic MTL model: illumination 0/1/2, ambient/diffuse/
specular/emissive factors, exponent, dissolve and supported maps/transforms are
consumed without guessing metallic or roughness values. Reflection/refraction
illumination models, halo dissolve, displacement, 3D texture coordinates and
simultaneous bump/normal maps are explicitly refused. Texture color/data roles
are distinct even when they reference the same encoded image.

Opaque and mask draws write Depth32Float; blend draws test depth without writing
it and use premultiplied source-over. Blend primitives sort stably far-to-near by
their transformed bounds centre. This is not order-independent transparency for
intersecting triangles. Mirrored transforms select the corresponding front-face
winding. Missing tangents use the signed UV Jacobian; discarding its sign would
reverse the mapped light direction. Normals use inverse-transpose transforms.

Healthy repeated frames do not log. Cold pipeline compilation emits Info start;
slow successful frames use Debug after 100 ms. Actual frame errors log on the
first occurrence and occurrence 60, suppress subsequent repetitions, and reset
on a successful frame. This instance-local consecutive counter does not measure
seconds. Frame errors keep their original return values.

## Explicit transform animation, cameras and shared native depth

`AnimationSample` selects a clip and time without autoplay, looping or blending.
TRS channels support STEP, LINEAR and time-scaled CUBICSPLINE; rotation LINEAR
uses shortest-arc quaternion interpolation. Endpoint values clamp to the clip's
key range. Authored matrices on nonanimated nodes and project multi-turn Euler
tracks remain independent. The project `ModelClip.animation` stores explicit
clip, offset and signed speed, sampled from layer-local frames and project FPS.

`Scene::camera_view` explicitly selects a reachable camera node, samples hierarchy
and transform animation, ignores camera scale and converts metres to pixels.
Finite/infinite perspective and orthographic views use native [0,1] depth.
Authored aspect is retained through a centered viewport. Collapsed, reflected or
sheared global camera bases are explicitly refused by the current sampling
profile. There is no automatic composition-camera replacement or selector UI.

`SceneFrame::from_imported_camera` and `ScenePassFrame::from_imported_camera`
consume these views. `ScenePass` accepts multiple owned `GpuScene` sources and
repeated instances, clearing one shared color/depth target once. Common camera,
viewport and 1x/4x sampling are explicit. Per-instance world, lighting and clip
samples remain independent. Opaque/masked primitives precede globally sorted
blended primitives; intersecting transparency still needs a separate OIT policy.
Targets and immutable uniforms use the shared allocation budget and retain their
actual encoder/submission lifetime, including abandoned command retirement.

`ScenePass::depth()` is hardware projected depth, not a metric or relative
`DepthResource`. Its single-sample texture permits exact native readback; there
is no implicit multisample-depth resolve or AI-depth calibration.

Project model layers still rasterize separately. The shared native pass does
not complete mixed mesh/plane/BSP, nested/matte/effect composition or calibrated
AI depth. Skin/morph support and camera-selection UI also remain outside this
checkpoint. Standalone native acceptance is distinguished from the separate
saved-project/editor/export evidence; #87 remains open.
