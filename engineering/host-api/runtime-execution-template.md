# Package-local image execution templates

Status: accepted in the targeted source-bound runtime-template checkpoint for
issue #78. The [delivery ledger](../validation/typed-render-dag-current.md)
records actual tests, retained failures and source/archive verification. This
does not complete the render DAG issue; all Android work is excluded by user instruction.

An image effect may declare `execution` and the required host capability
`image_execution_v1` together. Ordinary independently authored effects may omit
both and retain their existing pass-list semantics. A missing declaration,
unknown template version or unsupported capability is an error. No legacy
project migration is introduced.

The authoring document, this reusable execution template and a concrete frame's
resource/task plan have separate ownership. The template identifies package
operations and dependencies. It contains no frame layer IDs, device IDs,
texture handles, sampled numeric values or fixed viewport dimensions.

## Declaration and code binding

| Field | Meaning |
| --- | --- |
| `version` | Exact execution-template protocol version, currently 1 |
| `required_capabilities` | Capabilities required by its logical graph; the current image consumer supports `image_dag_v1` |
| `working_space`, `alpha_mode` | Must match the containing image effect |
| `precision` | `working`: bind the current project/channel precision at instantiation |
| `resources` | Exact ordered package resource paths, matching the effect's existing bindings |
| `nodes` | At most eight shader operations, one for each declared pass |
| `output` | ID of the one terminal operation |

A node has a package-local `id`, a `pass` index, `input`, `source` and the actual
`code_identity`. An input reference is either `{ "kind": "input" }`, the
effect's working image at entry, or `{ "kind": "node", "id": 10 }`, another
operation's output. `input` and `source` bind the SDK's primary and original
source sampling ports respectively. Neither vector order nor the manifest's
shader-list order establishes execution order.

Node IDs must be unique. Every declared shader pass has exactly one node, every
reference must resolve, and every operation must contribute to the terminal
output. Cycles, unused operations and repeated pass bindings are rejected.
The existing typed image execution checker supplies topology, value contracts,
lifetimes, capability checks and logical cost limits.

The identity is generated from actual compiled WGSL, GLSL, reflection
names/bindings, sprite/blend flags and static loop work. It excludes sampled
values and origins. It is not a publisher signature or a shader-equivalence
proof. The package loader validates its manifest before parsing shaders, then
compares identities against the compiled implementations. Native preparation
also compares actual programs, using the bounded weak code-identity cache.
Publicly replacing shader code cannot make an old template valid.

The graph compiler creates its single compiled image-operation template after
successful portable compilation. Numeric edits can retain the same template,
code and package. The complete authoring graph remains in the editable source;
it is not reconstructed by the runtime consumer.

## Actual native consumption

At preparation time the consumer binds the current origin, project precision,
input extent and evaluated output extent. The checked graph determines native
shader order and both sampled image bindings. Actual shader pass indices
remain in the SDK pass clock, even when topological order differs from the
manifest's shader-list order.

Working-image placement is separate from that logical graph. The current
native consumer admits three simultaneous working images in the effect's
existing color/alpha domain. The imported image occupies one until its final
use. An output cannot overwrite either sampled attachment on the same pass,
including when the sampled value dies at that pass. Values may release their
slots afterward. The established canonical output/conversion path and physical
device dimensions, scratch accounting and memory admission still apply.

An admissible diamond can prepare its shared upstream once and consume both
branches. A graph exceeding that working set returns typed `TemplateError::Storage`
with the node and required/available capacities. It does not add retries,
silently linearize the graph or drop a branch. Existing effect diagnostics add
layer/effect context and preserve the original error. Template and structural
execution errors must be returned even by ordinary non-strict previews. The
previous effect-bypass policy would otherwise turn a refused graph into a
different successful image. Ordinary effects without such execution errors
retain their existing preview policy. This is a functional refusal rule, not
just an added observation.

Package-level shader costs count unfused operations and exclude host raster and
conversion utilities. Unknown sampling bounds remain unknown. Native pass
counts, measured timings and physical peaks are different quantities; logical
costs do not establish frame speed or driver memory consumption.

## Boundaries

Template contracts currently describe image programs in one effect's working
color/alpha domain. Ordered resource paths retain existing package PNG and
`shader_data_v1` admission. They do not yet provide fully typed generic external
image/LUT/mask producers. History, stateful execution, side effects and spatial
producers are not supported by this image-template consumer.

The packed per-frame ABI is the separate frozen Android/GLES adapter contract.
Packed v12 continues to explicitly reject native private-data texture tokens
instead of exporting stale package defaults. On 2026-10-25 the user explicitly
excluded all Android implementation and acceptance because Android is no longer
maintained. Extending that retired adapter is not a maintained-desktop delivery
requirement. Old project migration is also outside scope. Desktop package/native
runtime version and capability refusal remain mandatory.

The targeted runtime checkpoint includes four-precision private-instance
acceptance: 16 raw-pixel numeric updates and eight disk reopen/isolation/undo
cases. Complete initial semantic lowering before CPU preparation, fully typed
external imports, bounded parallel preparation, end-to-end cost measurement,
complete IR fault output and final integrated/cross-platform verification remain
open in the delivery ledger.
