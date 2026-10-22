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

Query and update responses contain `policyVersion: 4` and `memoryPolicy`.
An update supplies the policy object:

```json
{"op":"memory_policy","policy":{"sessionAllocationBudgetBytes":536870912,"sourceTextureBudgetBytes":268435456,"scratchBudgetBytes":100663296,"previewTargetBudgetBytes":33554432}}
```

| Field | Accepted range | Default | Accounting scope |
| --- | --- | --- | --- |
| `sessionAllocationBudgetBytes` | 1 byte..4 GiB | 512 MiB | Shared logical capacity of enrolled native allocations across session renderers and frozen outputs |
| `sourceTextureBudgetBytes` | 4 bytes..2 GiB | 128 MiB | Existing source/working-set checks |
| `scratchBudgetBytes` | 64..384 MiB | 64 MiB | Effect scratch |
| `previewTargetBudgetBytes` | 8 bytes..128 MiB | 128 MiB | One retained front plus its candidate |

The session, source and scratch fields are required. The existing omission rule
for `previewTargetBudgetBytes` remains 128 MiB. Unknown fields and out-of-range
values reject the entire update before changing runtime policy. The automatic
`configure_memory(totalMem, guarded)` entry point keeps its original version-1
response and scratch selection rules, and preserves explicit session, source and
preview-target allowances. System RAM is not reported as GPU memory capacity.

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
Complete integration of the shared admission refusal with every preview retry
and proxy stage still needs its own acceptance.

Frozen PNG and movie outputs capture project, effects, assets and category policy
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

File decode, package and device safety limits remain in force. Coverage is
explicitly incomplete: presenter/UI resources, CPU caches, decoder memory,
swapchain images and driver overhead are not all enrolled. Logical admission
across devices still does not measure physical VRAM. Read-only native adapter budget metadata is exposed by the
[platform pressure API](platform-memory-pressure.md); platform execution limits
are recorded separately in the validation document. Pressure-driven admission,
combined frame-peak accounting, the complete bounded preview/proxy loop and
actual presentation lifecycle acceptance remain pending. The packed render-plan
version and Android GLES consumer ABI are unchanged by this API increment.
