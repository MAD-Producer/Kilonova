# Session resource budget API

This API configures application-owned logical allowances. It does not query or
declare available GPU memory. Policy is session runtime state: it is not saved
in project data and does not create undo entries.

The Rust entry point is
`motion_host::ops::preview::configure_memory_policy(id, MemoryPolicy)`.
The shared desktop/Android plugin JSON channel supports a read-only policy query:

```json
{"op":"memory_policy"}
```

The supported build targets are desktop Windows, Linux and macOS. Android is
retained historical source and is no longer maintained or released, as recorded
in README. Its construction signatures are propagated here, but a manual arm64
cross check still fails at the adapter's String/typed-error boundaries. This API
increment does not claim a passing Android build or Android device acceptance.

Query and update responses contain `policyVersion: 5` and `memoryPolicy`.
An update supplies the policy object:

```json
{"op":"memory_policy","policy":{"sessionAllocationBudgetBytes":536870912,"sourceTextureBudgetBytes":268435456,"decodedImageBudgetBytes":201326592,"scratchBudgetBytes":100663296,"previewTargetBudgetBytes":33554432}}
```

| Field | Accepted range | Default | Accounting scope |
| --- | --- | --- | --- |
| `sessionAllocationBudgetBytes` | 1 byte..4 GiB | 512 MiB | Shared logical capacity of enrolled native allocations across session renderers and frozen outputs |
| `sourceTextureBudgetBytes` | 4 bytes..2 GiB | 128 MiB | Existing source/working-set checks |
| `decodedImageBudgetBytes` | 4 bytes..2 GiB | 128 MiB | Per-image CPU RGBA8 output; full-source capacity for non-streaming codecs |
| `scratchBudgetBytes` | 64..384 MiB | 64 MiB | Effect scratch |
| `previewTargetBudgetBytes` | 8 bytes..128 MiB | 128 MiB | One retained front plus its candidate |

The session, source and scratch fields are required. The existing omission rule
for `previewTargetBudgetBytes` remains 128 MiB. Omitting the optional decoded
image allowance selects its conservative 128 MiB default. Unknown fields and out-of-range
values reject the entire update before changing runtime policy. The automatic
`configure_memory(totalMem, guarded)` entry point keeps its original version-1
response and scratch selection rules, and preserves explicit session, source and
preview-target and decoded-image allowances. System RAM is not reported as GPU memory capacity.

Each Session owns one AllocationBudget. Desktop and Android surface renderers,
host plugin-editor renderers, PNG capture and frozen movie export receive the
same identity before native renderer construction. `Platform::attach_surface`
therefore takes an AllocationBudget; `Session::attach` rejects a different
identity before detaching the current surface. Standalone Renderer callers supply
their own allowance at construction; `Renderer::set_memory_policy` changes the
category checks and does not reset that shared allowance.

Lowering the session allowance does not discard existing allocations or pretend
that submitted resources have retired. Existing owners remain booked; growth is
refused while the combined admitted capacity would exceed the current allowance.
Releasing an owner frees its capacity only after all relevant aliases, encoding
scopes and submitted work release their leases. Replacing a project preserves
the Session's admission identity and lifetime peak.

The source allowance covers the existing image, video-plane, plugin, vector,
mask, layer-snapshot, nested-composition and compositing category checks. Scratch
retains its independent limit. These checks remain in addition to native
admission. Typed resource refusals preserve `requiredBytes`, `budgetBytes` and
the resource category. They do not classify every GPU/shader/codec error as a
resource-pressure condition.

Preview target admission includes the retained front when preparing or reusing a
candidate, and checks before allocating or replacing it. Capacity uses the actual
color format, including eight-byte RGBA16Float. The existing per-target
16-megapixel safety limit and device dimensions remain independent constraints.
An unrendered host placeholder starts at one pixel; the actual requested frame
is prepared under the existing finite degradation/cooldown policy. Refused
candidates do not overwrite the submitted front. The preview snapshot reports
requested/actual quality, rejection reason and whether a previous frame is held.
The cumulative implementation combines refusal-only adaptation with the separate
native-pressure floor, idle-first eviction and existing proxy/target controls.
Its full preview/lifecycle acceptance remains pending; implementation presence
does not inherit earlier-source passing results.

Frozen PNG and movie outputs capture project, effects, assets and category policy
including the CPU decoded-image allowance
at job creation. Their shared native allowance remains live: concurrent preview
allocations and subsequent session allowance updates still apply. An old frozen
category policy cannot reset the newer shared limit. A frozen job keeps admission
alive after Session closure. Formal output preserves full dimensions and sampling;
the preview target allowance does not reduce its capture targets. Insufficient
capacity produces an explicit error and leaves the previous published file intact.
There is no added export downscaling, retry or silent effect removal.

The plugin bridge exposes bounded admission metadata without a surface:

```json
{"op":"allocation_budget"}
```

```json
{"version":1,"scope":"session_enrolled_native_capacity","allocationBudget":{"budgetBytes":536870912,"admittedBytes":0,"peakAdmittedBytes":0,"liveAllocations":0,"encoderScopes":0},"isFramePeak":false,"isDriverResidency":false,"completeCoverage":false}
```

`Session::allocation_budget_info()` returns the same metadata fallibly. The peak
is the admitted-capacity high-water mark since Session creation, including
concurrent renderers and retained work; it is not a per-frame peak, GPU residency
measurement or available-memory query. Querying it performs no resource walk or
GPU poll and does not change project/revision/undo state. Broken accounting state
returns an error rather than a fabricated empty result. The older
`resource_ledger` query remains a separate ownership observation.

The desktop's independent node preview also enrolls its sample image, LUT,
uniforms and full-resolution floating-point output through the renderer's shared
[native allocation context](native-allocation-context.md), retaining queued writes
and command reservations until retirement. Failed replacement retains the last
valid graph; restoring the allowance permits an ordinary full-quality install.

## Separate limits and current coverage

The shared admission limit and source/scratch/preview allowances describe logical
native GPU capacity. Adapter budget/usage describes advisory platform pressure;
neither is an available-memory guarantee and neither is obtained by subtracting
the other. The [platform pressure API](platform-memory-pressure.md) describes
separate per-pool comparisons, unknown readings and preview-only transitions.
The [frame allocation API](frame-allocation-peaks.md) describes current, lifetime
and simultaneous interval peaks, including fixed resource categories and queue
retirement endpoints. The lifetime peak on `allocation_budget` does not become a
frame peak when that separate API is enabled.

File decode, package and device safety limits remain in force. In particular,
`image_resources::MAX_ENCODED_BYTES` still bounds encoded image inputs at 64 MiB.
Session rendering, `asset_pixels`, `asset_pixels_into`, and frozen PNG/movie
output use `decodedImageBudgetBytes` for CPU RGBA8 results. Standalone `decode`
and `decode_into`, and the pre-registration `prepare_image` probe, retain the
128 MiB default because they have no Session policy. Explicit standalone callers
can use `decode_with_budget` and `decode_into_with_budget`.

The decoded-image allowance is independent of GPU texture admission. Increasing
`sourceTextureBudgetBytes` does not silently increase CPU allowances. A proxy
checks its own RGBA8 output capacity before reading the cache; PNG streams the
source without a full RGBA image, while JPEG also checks full-source capacity.
Result allocation reserves fallibly. Policy refusals carry the typed
`decodedImages` scope; invalid metadata, wrong output lengths, file/codec errors,
cancellation and allocation failures retain their original identity.

Changing the CPU allowance invalidates pending image decodes and speculative
failure suppression. A stale result is discarded even if decoding completed
before cancellation; the single task slot stays occupied until it returns.
Resident GPU images and submitted preview frames are retained. This is a
per-image result/full-source bound, not a total bound on concurrent outputs,
codec workspace, proxy sampling rows or process RAM. The legacy `asset_pixels`
entry point preserves its existing conversion/premultiplication semantics and
permits a bounded 16-bit codec intermediate independently of the RGBA8 result.
Implementation and actual acceptance are tracked separately in
[decoded-image budget validation](../validation/decoded-image-budget.md).

Coverage remains incomplete: Presenter/UI resources, CPU caches, decoder memory,
swapchain images and driver overhead are not all enrolled. Raw external resource
and submission paths are outside the admitted-wrapper contract. Category totals
and completed frame windows do not make `completeCoverage` true. Logical
admission across devices still does not measure physical VRAM.

Pressure consumption, category/frame peaks and repeated native lifecycle guards
are implemented in the cumulative candidate, with acceptance tracked in
`engineering/validation/frame-memory-admission.md`. Their runtime and platform
limits must be verified on that source before claiming delivery. Earlier base
and export acceptance is recorded separately in
`engineering/validation/frame-export-allocation-windows-completed.md`. The packed
render-plan version and Android GLES consumer ABI are unchanged by these budget
observation/policy increments. Android remains unmaintained; no Android runtime
acceptance is implied by retaining historical source signatures.
