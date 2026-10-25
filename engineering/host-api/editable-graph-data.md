# Editable graph numeric data, version 1

This is a partial #78 implementation. It separates numeric values from shader
text in the live compilation path. It does **not** yet avoid all portable shader
compilation, package replacement or native/preview pipeline creation.

## Entry points and ownership

`motion_graph::compile` and `compile_preview` produce baked shader source.
`compile_editable` and `compile_preview_editable` produce code plus private data.
Desktop `EditorSnapshot` exposes the same distinction through `compile_runtime`
and `compile_preview_runtime`; the actual `NodeTasks` compilation worker uses
these runtime entry points, including canonical bound-layer compilation.
Ordinary snapshot compilation and offline export retain their baked contract.

Editable Float, Vec2, Vec3, Color and Bool literals, unconnected input defaults,
and CPU-folded numeric results use structure-keyed data locations. Values never
enter their generated expressions or common-expression keys. Equal constants
becoming different therefore keep code stable. Existing CPU constant-folding
arithmetic and straight/premultiplied authoring rules are preserved. The same
authoring topology is the scope of this code-stability guarantee.

Private values are separate from public parameter definitions and controls.
They do not add controls to the editor. `CompiledGraph::package_bytes` includes
declared numeric resources when producing a project-owned runtime package;
`resource` provides read-only access for independent preview construction.

## Runtime declaration and admission

An effect must declare both `shader_data` and the `shader_data_v1` required
capability. They must agree. The declaration requires SDK 6 image bindings and
exact data version 1. It cannot overlap the parameter/resource reservations of
`image_input` or `param_lut`. Unknown versions and inconsistent declarations
are errors, rather than defaults or migration requests.

The declaration contains uniform tail cells and an optional overflow resource.
Tail cells start immediately after the public parameter cells; their combined
count cannot exceed the existing 32-cell uniform ABI. The native frame builder
and independent node preview both copy these cells into their actual uniforms.

The editable compiler admits at most 16,384 private scalar components. Values
must be finite and have absolute value at most 1e12, including folded results.
Existing document/literal, source, loop, sampling, package and resource limits
remain in force. Private data does not raise any of those limits.

Overflow occupies resource zero, `assets/graph-data.png`, with width 256 and
height `ceil(scalars / 256)`. Each RGBA8 pixel holds one little-endian f32 bit
pattern. The host uploads it as linear `Rgba8Unorm`; `textureLoad` and byte
reconstruction recover its bits without filtering or color transfer. This is
a byte container, not an 8-bit scalar representation. The loader checks exact
dimensions, RGBA8 type and finite bounded decoded values in addition to normal
package/decoded-resource admission. Image failures retain their native causes.

Independent preview uses the same admitted texture allocator and keeps the
numeric texture's ownership until the preview is released. Package loading,
registry insertion and saved runtime restoration keep their existing owners.
No node gets a separate GPU device or submit operation.

## Owned compilation and preview reuse

`EditableCompiler` owns at most two portable compiler results, for canonical
and diagnostic outputs, with an additional 8 MiB retained UTF-8 code limit.
The actual node compiler thread owns this cache; there is no global cache.
The entry point and SDK wrapper are fixed by this API. Lookup compares the
complete generated source, after current graph validation and lowering. A hit
returns a clone of the compiler's private immutable result; mutating a returned
`CompiledGraph.shader` cannot change the entry. Source, sampling, loop, manifest
and numeric-data budgets still apply to each request. `clear()` releases entries
and retained code while lifetime call/reuse counters remain cumulative.

`CompileReport.portable_compilations` counts actual portable compiler calls for
that request. `compiler_cache_code_bytes` and the owner's stats report retained
UTF-8 source, WGSL, GLSL and reflection-name bytes. They exclude container
overhead, returned graph copies, other owners, native drivers and GPU resources;
these fields are not a whole-process memory measurement.

`NodePreview::build_reusing` constructs a fresh admitted candidate. It shares
the successful graph pipeline only when complete actual WGSL and the private
allocation/upload owner match. This preview has fixed SDK bindings, fixed entry
points, a fixed Rgba16Float graph target and no blending specialization. The
fixed presentation pipeline is reused only for the same owner and target
format. It retains the current format plus at most one alternate, normally
the native swapchain and PNG capture formats. A third format evicts the older
alternate; switching to a retained format does not create a pipeline. New
uniforms, input/data/output textures, presentation buffer and bind
groups remain separate from the old candidate, and the paused output cache
starts empty. Failed compilation, validation, admission or subsequent layer
application therefore cannot mutate the previous candidate's pixels.

The successful graph artifact retains at most 2 MiB WGSL for full comparison.
It is shared with the candidate instead of accumulating historical versions.
Manifest/data checks run even on a pipeline hit. Changed or malformed actual
WGSL takes the ordinary native validation path. Adapter names, graph IDs,
package hashes and the public mutable compiler result are not reuse proofs.

Desktop state exposes the active candidate's `candidatePipelineBuilds` and
`retainedShaderCodeBytes`. The build count includes its graph and presentation
pipeline creations, including later presentation-format changes. It is local
to this candidate, not a process-wide total or a count of completed GPU work.
The retained code gauge excludes the driver and other live candidates.

## Costs and current limits

Overflow adds shader texture-load sites and a resource upload. These loads count
toward the existing static sampling-site budget; they are not free. The compile
report's resource byte field counts encoded PNG bytes, not GPU allocation or
resident memory. Ordinary resource/allocation accounting remains authoritative.
No frame-rate improvement or hardware bus-traffic reduction is established.

Bound instances now hold immutable, typed `ShaderDataValues` separately from the
validated code package. Project format 27 deliberately refuses older consumers
that could ignore these values. The existing serde `rc` feature serializes the
payload; no dependency version changes. Deserialization bounds sequences before
allocation (32 vec4 cells and a combined 16,384-scalar budget). Values must be
finite, bounded and match the package's exact declaration. Equality preserves
floating-point bits, including signed zero. Sampled scenes share the payload
Arc; Present does not parse JSON or decode its PNG representation.

Bound application compares the full static manifest, actual source/WGSL/GLSL,
reflection, specialization flags and companion resource bytes before retaining
the existing package Arc and embedded archive. Public defaults and private
numbers initialize the instance separately. Actual code/layout changes keep the
ordinary package validation/install path. This does not yet preserve unrelated
native programs across structural package changes.

Uniform overrides use the declared tail. Native overflow uses a frame-local
token and an admitted resource-zero texture. Companion resources remain intact.
The cache retains current passes only, uploads changed values, and includes old
plus candidate texture capacity during admission. Logical GPU bytes and upload
bytes are accounted; every encoded use retains its reservation through GPU
submission completion. Package-default textures can remain cached too and are
included in the total. Packed plan v12 explicitly refuses per-instance overflow
instead of silently using package defaults. Uniform-only overrides already fit
its ordinary uniform block. A versioned portable overflow/execution-plan
consumer remains unfinished.

Canonical diagnostic validation now retains only successful owner-local
pipelines, bounded WGSL and the two presentation variants. Its candidate data,
textures, buffers and groups retire after validation; failed candidates cannot
replace the active preview or canonical proof. Desktop state exposes separate
canonical candidate build and code gauges. These local gauges are not total
driver memory or completed GPU compilation counts.

Complete pre-placement effect IR, structural affected-group reuse, bounded
parallel CPU preparation, versioned execution-plan consumers, final cost/peak
measurements and integrated/cross-platform checks remain pending. The current
bound pixel matrix is Unorm8; earlier independent transport precision tests do
not substitute for all-precision per-instance acceptance.
Android device acceptance is excluded by the user; no device result is claimed.

No new dependency version or third-party asset is introduced. `motion-graph`
uses its existing workspace `image` dependency in production as well as tests.
Private PNG data is generated from numeric values by independently authored code.

Source-bound results and retained failures are recorded in
[the current delivery ledger](../validation/typed-render-dag-current.md).
