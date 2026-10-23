# Image decode worker, scratch and completion CPU observation

Issue: #60. `DecodeTask::memory_info()` is an explicit non-consuming query.
`Renderer::image_decode_memory()` exposes its owned task. Session's existing
`resource_memory` read adds `cpu.imageDecodeTasks`, covering the original-input
reader, preview renderer and editor renderer if present. This domain contributes
to CPU known payload/capacity once, separately from retained source-pixel caches,
GPU textures and logical admission. It is not process RAM or driver residency.

## Actual ownership

Each task has exactly three fixed buffer slots: output RGBA8, pixel accumulators
and the JPEG decoder's returned DynamicImage. The latter counts actual backing
Vec capacity for its supported byte/16-bit/float variants. A future unknown image
variant preserves payload but leaves capacity null. PNG codec-internal storage,
JPEG codec workspace, source readers, proxy metadata, allocation overhead and
retired task handles remain explicit unknown/excluded domains.

A slot becomes unknown before allocation. After actual allocation it reports
Vec length and capacity, independently of requested dimensions and budget. The
real storage and its private lifetime lease move together. Output persists
through decode and the bounded completion channel; scratch drops at its actual
scope. Fields drop real storage before their observation lease. Failure, unwind
and a disconnected result receiver release these leases with their actual data.

`cancelRequested` does not clear allocation counts. A cancelled completed result
can remain queued until its real consumer polls or its owner is destroyed. Drop
of a task handle does not join the worker or prove a retiring worker freed its
codec buffers. No global registry or new lifetime owner retains pixels.

`poll` retains its existing result/cancellation/error semantics. It has exclusive
access to the owning task. Moving output into a retained Reader cache or GPU
upload cannot race a borrowed Session memory query, so that query does not add
both task output and its transferred cached allocation. An escaped returned
pixel packet is outside the task's scope and remains usable after its count
leaves the task domain. Distinct tasks with the same source ID own independent
allocations and must be added rather than deduplicated by source.

## Bounds and coherence

Metadata retains only three fixed slots and static phases for one task. Context
is numeric source ID, resolution, budget, speculation/cancellation state and
elapsed time. Paths, pixels and addresses are not returned. Metadata writers
only update fixed fields. The query uses `try_lock`, copies fields, releases the
guard, and only then constructs JSON. Busy/poisoned observations report partial
known subtotals with unavailable context, null unknown bytes and
`allInstrumentedBuffersObserved=false`; they never mean zero actual memory.
A new task generation starts with fresh metadata. Multiple tasks/domains are
sampled sequentially, not atomically, and this is not a per-frame peak probe.

Queries do not read files, poll/drain result channels, join workers, request
frames, initialize GPU state or edit project/history/budgets. Ordinary preview
snapshots do not run this census. No logger runs while an observation guard is
held. Existing source, cancellation, panic and error reporting are preserved.

The earlier source709de340 performance archive has no original-input decode task
and is not relabeled as acceptance for this domain. Actual correctness commands,
controlled worker transitions, native PNG/JPEG/Adam7 pixels and public Session
ownership transitions require separate source-bound acceptance records. Real
file-video/window/pressure/physical-mobile and before/after studies remain #60
requirements beyond this CPU instrument.
