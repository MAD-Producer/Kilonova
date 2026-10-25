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

## Costs and current limits

Overflow adds shader texture-load sites and a resource upload. These loads count
toward the existing static sampling-site budget; they are not free. The compile
report's resource byte field counts encoded PNG bytes, not GPU allocation or
resident memory. Ordinary resource/allocation accounting remains authoritative.
No frame-rate improvement or hardware bus-traffic reduction is established.

Next work includes per-instance private data updates without code-package churn,
bounded compiler ownership and actual compilation/pipeline counter acceptance,
complete pre-placement effect IR, versioned execution-plan consumers, precise
default failure-output acceptance and final integrated/cross-platform checks.
Android device acceptance is excluded by the user; no device result is claimed.

No new dependency version or third-party asset is introduced. `motion-graph`
uses its existing workspace `image` dependency in production as well as tests.
Private PNG data is generated from numeric values by independently authored code.

Source-bound results and retained failures are recorded in
[the current delivery ledger](../validation/typed-render-dag-current.md).
