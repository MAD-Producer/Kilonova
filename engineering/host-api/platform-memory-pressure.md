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
observation mutex is held while calling the logger. Preview pressure polling is
gated as described below; a healthy frame does not make a native call or write a
log on every invocation. A serial worker still publishes its existing in-flight/
start observation before handling an explicit non-Present request.

Hosts that call the renderer inside a business lock bind `MemoryPressureLogScope`
before acquiring that guard. `with_session` does so for the shared host bridge.
Normal return and unwind release the session guard before emitting observations.
The scope is pinned to its calling thread, supports nesting and retains at most
one first Debug and one first Error cause, with a saturating omitted count. It
releases its thread-local observation borrow before invoking the logger.

The metadata query itself is read-only: it does not reclaim caches, alter the
configured admission limit, retry an output or modify a project. The attached
preview consumer below uses these same advisory readings separately.
Tests and platform acceptance are recorded separately; a compiled probe is not
proof that a particular driver advertises the optional Vulkan extension.

## Bounded preview consumer

Attached ordinary, exposure and retained preview paths consult the existing
renderer at most once per native minimum interval (currently one second).
Interval arithmetic retains full `Duration` precision. A process-local native
sample identity also prevents re-counting a cached reading when Session and
probe clock reads straddle a deadline. This identity is not serialized. A
metadata query may refresh the probe, but it does not directly advance preview
failure counters. No renderer is constructed solely for this policy.

For each nonzero-budget native pool, compare usage/budget independently with
checked-width integer arithmetic. Never sum dissimilar pools into free VRAM or
subtract the application ledger from a native estimate. Empty or zero-budget
pool results are unknown and cannot authorize a quality increase.

| Reading / action | Preview-only behavior |
| --- | --- |
| First sample at or above 90% in an episode | Evict idle image entries and idle CPU video proxies; preserve active inputs, pending conversions and the completed front; suspend speculative image prefetch |
| Another high sample after eviction | Raise the native quality floor one level, then at most once every five seconds, bounded to the existing four quality levels |
| Intermediate usage above 70% and below 90% | Hold the current floor and reset consecutive high/low streaks |
| Usage at or below 70% | Lower the floor one level only after five low samples, 120 successfully rendered frames, and a 30-second cooldown since the last high sample or prior recovery |
| Unavailable reading | Retain the current floor, reset streaks, expose the unavailable reason; do not invent a successful fallback |

The effective requested quality is the stricter of the existing typed-refusal
level and the native floor. Existing image/video proxy and intermediate-target
controls implement that quality; no effect is disabled. A floor change cancels
an obsolete exposure request, discards only the candidate, invalidates retained
reuse and clears CPU proxies. Last submitted pixels remain available until a
replacement is ready. A retained presentation re-enters the ordinary render
path if its effective quality changed, avoiding a permanent retained bypass.

Native advice never manufactures a `ResourceRejection`, changes configured
allowances or hides shader, codec, cancellation or device failures. Typed budget
refusals still follow the existing bounded retry and rollback contract, starting
above the effective floor. Formal frozen PNG/movie outputs and full-resolution
capture do not consume the preview floor. Project state and undo remain intact.

`memoryAdaptation.nativePressure` exposes fixed-size policy context: status,
native floor, source, worst pool, unavailable reason, bounded high/low streaks
and next sample/change delays. Existing `requested` and `actual` preview quality
remain separate. An observed eviction is logical cache ownership removal, not
proof that queued GPU aliases or the driver released physical VRAM.

The first high sample logs Debug; consecutive distinct high samples saturate at
60 and log Error once at 60. Low, intermediate or unavailable readings reset
that episode. Counts do not imply a fixed duration. Floor changes log Warn at
the bounded transition cadence. Registered Session calls defer these records
past business guards, including unwind, retaining at most three events and an
omission count. The sink is called after the observation borrow is released.
Application Error routing remains file plus stderr; healthy Present is quiet.

This consumer is a functional preview policy change, separate from the original
read-only observation increment. Its tests, source and platform limits must be
accepted independently before claiming delivery.

Primary references used for independently authored bindings integration:

- [Microsoft DXGI query](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_4/nf-dxgi1_4-idxgiadapter3-queryvideomemoryinfo)
- [Microsoft budget and usage fields](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_4/ns-dxgi1_4-dxgi_query_video_memory_info)
- [Khronos memory budget extension](https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_memory_budget.html)
- [Apple recommended working set](https://developer.apple.com/documentation/metal/mtldevice/recommendedmaxworkingsetsize)
- [Apple current resource allocation size](https://developer.apple.com/documentation/metal/mtldevice/currentallocatedsize)
- [wgpu guarded native adapter access](https://docs.rs/wgpu/30.0.1/wgpu/struct.Adapter.html#method.as_hal)
