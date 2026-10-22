# Read-only platform memory pressure

The renderer probes the native adapter it already owns. It does not enumerate a
different GPU, estimate VRAM from system RAM, create a GPU for a metadata request,
or modify the application's configured admission limit. These advisory native
budgets are separate from `allocation_budget` and its enrolled logical capacity.

The existing plugin JSON operation accepts:

```json
{"op":"memory_pressure"}
```

The response contains `memoryPressure`, also available through
`Session::memory_pressure_info()` and `Renderer::memory_pressure()`:

- `version`: 1
- `backend`: the actual wgpu backend, or `unattached` when no renderer exists
- `ageMs`: age of a cached native reading
- `minimumQueryIntervalMs`: 1000, per renderer instance
- `isPhysicalCapacity`: false
- `isFramePeak`: false
- `reading`: a tagged available or unavailable result

Available readings contain a `source` and bounded `pools`. Each pool has `index`,
`budgetBytes` and `usageBytes`. A legitimate zero native budget is preserved,
including usage above that budget. The pools are not added together into a
claimed free-VRAM value.

| Backend | Source | Scope and limits |
| --- | --- | --- |
| D3D12 | `dxgi_local_segment` | DXGI node 0 local memory segment; OS budget and current application usage |
| Vulkan | `vulkan_device_local_heaps` | At most 16 device-local heaps; implementation-defined process usage estimates and advisory budgets from `VK_EXT_memory_budget` |
| Metal | `metal_working_set` | Existing MTLDevice's recommended working set and current resource allocation size |

Unavailable readings contain a `reason` and optional `cause`, with no fabricated
budget/usage pools. Reasons distinguish no active renderer, unsupported backend,
missing HAL adapter, missing Vulkan budget extension or properties entry point,
no device-local heap, an invalid native result, and a native query failure.
Retained native failure causes are bounded to 512 Unicode scalar values.

Session queries use the attached surface renderer first, otherwise the existing
headless plugin-editor renderer. A Session with neither remains unattached;
querying it does not initialize a renderer. Other frozen outputs or independent
renderers are not independently enumerated by this Session operation.

Both successful and unavailable readings are cached. Repeated callers do not
poll the native API or advance failure counters until the interval expires.
Refreshing after a native query failure replaces the prior successful reading
with unavailable; stale success is never used as a healthy fallback. The counter
is scoped to consecutive actual native query failures in one renderer instance:
Debug on the first, Error once at 60, saturating at 60. Success or an ordinary
unsupported/missing-extension boundary resets it. A cached query does not reset
or increment it. Counts do not imply a fixed wall-clock duration.

Native handles are borrowed only inside wgpu's HAL adapter guard. Queries are
read-only; no native object or reference escapes the guard, and no business or
observation mutex is held while calling the logger. No native polling is added
to Present or another healthy per-frame path. A serial worker still publishes
its existing in-flight/start observation before handling this explicit request.

Hosts that call the renderer inside a business lock bind `MemoryPressureLogScope`
before acquiring that guard. `with_session` does so for the shared host bridge.
Normal return and unwind release the session guard before emitting observations.
The scope is pinned to its calling thread, supports nesting and retains at most
one first Debug and one first Error cause, with a saturating omitted count. It
releases its thread-local observation borrow before invoking the logger.

This API observes pressure; it does not yet connect native budgets to preview
reclamation, proxy size, admission limits or output retries. Missing queries keep
the existing conservative application policy. Preview/export dimensions, quality,
project values and undo history remain governed by their existing code paths.
Tests and platform acceptance are recorded separately; a compiled probe is not
proof that a particular driver advertises the optional Vulkan extension.

Primary references used for independently authored bindings integration:

- [Microsoft DXGI query](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_4/nf-dxgi1_4-idxgiadapter3-queryvideomemoryinfo)
- [Microsoft budget and usage fields](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_4/ns-dxgi1_4-dxgi_query_video_memory_info)
- [Khronos memory budget extension](https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_memory_budget.html)
- [Apple recommended working set](https://developer.apple.com/documentation/metal/mtldevice/recommendedmaxworkingsetsize)
- [Apple current resource allocation size](https://developer.apple.com/documentation/metal/mtldevice/currentallocatedsize)
- [wgpu guarded native adapter access](https://docs.rs/wgpu/30.0.1/wgpu/struct.Adapter.html#method.as_hal)
