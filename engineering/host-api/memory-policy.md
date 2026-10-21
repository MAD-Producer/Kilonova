# Session texture budget API

This API configures application-owned logical texture allowances. It does not
query or declare available GPU memory. Policy is session runtime state: it is
not saved in project data and does not create undo entries.

The Rust entry point is
`motion_host::ops::preview::configure_memory_policy(id, MemoryPolicy)`.
The shared desktop/Android plugin JSON channel supports a read-only query:

```json
{"op":"memory_policy"}
```

Query and update responses contain `policyVersion: 3` and `memoryPolicy`.
An update supplies the policy object:

```json
{"op":"memory_policy","policy":{"sourceTextureBudgetBytes":268435456,"scratchBudgetBytes":100663296,"previewTargetBudgetBytes":33554432}}
```

| Field | Accepted range | Default | Accounting scope |
| --- | --- | --- | --- |
| `sourceTextureBudgetBytes` | 4 bytes..2 GiB | 128 MiB | Existing source/working-set checks |
| `scratchBudgetBytes` | 64..384 MiB | 64 MiB | Effect scratch |
| `previewTargetBudgetBytes` | 8 bytes..128 MiB | 128 MiB | One retained front plus its candidate |

Older requests may omit `previewTargetBudgetBytes`; it defaults to 128 MiB,
preserving the previous maximum of two 16-megapixel RGBA8 targets. The source and
scratch fields remain required. Unknown fields and out-of-range values reject
the entire update atomically. The automatic `configure_memory(totalMem, guarded)`
entry point keeps its original version-1 response and scratch selection rules,
and preserves explicit source and preview-target allowances. System RAM is not
reported as GPU memory capacity.

The source allowance is shared by the existing image, video-plane, plugin,
vector, mask, layer-snapshot, nested-composition and compositing checks; this is
not one combined reservation allocator. Scratch retains its independent limit.
Lowering a policy also rechecks cached resources. Refusals preserve the typed
`requiredBytes`, `budgetBytes` and resource category.

Preview target admission includes the retained front when preparing or reusing a
candidate, and checks before allocating or replacing it. Capacity uses the actual
color format, including eight-byte RGBA16Float. The existing per-target
16-megapixel safety limit and device dimensions remain independent constraints.
An unrendered host placeholder starts at one pixel; the actual requested frame
is prepared under the existing finite degradation/cooldown policy. Refused
candidates do not overwrite the submitted front. The preview snapshot reports
requested/actual quality, rejection reason and whether a previous frame is held.

New preview and plugin-editor renderers inherit the policy. Full-quality PNG
and frozen movie export capture the session policy independently of later live
changes. The preview target allowance does not downscale formal output or limit
its capture targets. Source and scratch limits still apply to export and report
an explicit error rather than silently changing resolution or effect settings.
Accounting for float targets does not enable an end-to-end float composition or
readback pipeline.

File decode, package and device safety limits remain in force. This API does not
yet query platform pressure or cap the combined frame peak. The target allowance
covers one application's front/candidate pair, not swapchain images, exposure
resources, other renderers, buffers, CPU caches or decoder allocations. Dropping
an application handle does not prove immediate GPU retirement. The packed
render-plan version and Android GLES consumer ABI are unchanged.
