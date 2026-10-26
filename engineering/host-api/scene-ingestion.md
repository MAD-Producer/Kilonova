# Owned scene ingestion

`motion-scene` is the CPU ingestion foundation for issue #87. It is a workspace
library without GPU, window, inference, filesystem-at-render-time or Android
dependencies. No desktop import command or `Content` variant is advertised by
this change. Successful parsing does not establish PBR or 3D rendering support.

The independent implementation follows the [Khronos glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html),
sections 3.4–3.9 and 4, revision `8e798b02d254cea97659a333cfcb20875b62bdd4`.

## Static import profile

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

Animations, skins, morphs, cameras, nontriangle topology, unsupported attribute
semantics and required extensions produce explicit `Unsupported` errors. An
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
space also has +Y up. A later composition adapter must apply its camera/layer
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
