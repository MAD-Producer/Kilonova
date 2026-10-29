# Native composition frame cache (#82)

Status: the primitive cache was accepted at `45241fb31cdbbd0d87a30f4cd47c83435162b4ee`.
This increment is separate from the model-library checkpoint. It does not
enable `capabilities.prerender` or claim an integrated background renderer.

## Implemented source contract

`CompositionFrameCache` retains at most eight entries and an explicit byte
quota. It stores independent texture copies of completed composition pixels,
with the exact native texture format, dimensions, fractional frame bits and
document/device/content/output context. It performs no output conversion,
CPU readback, disk IO or UI drawing. The digest fields are caller contracts;
authoritative project/source key construction is still a host integration task.

Copy targets use the renderer's existing shared allocation admission. An LRU
entry is evicted before allocating a replacement. Entries retained by consumers
remain charged to that shared budget, including actual pending command encoder
and queue completion ownership. Cache resident bytes deliberately exclude those
evicted consumer handles; they are not a claim of released GPU capacity.

Disabled policy, explicit quota refusal, ordinary misses and stale generations
are observable normal outcomes, not rendering failures. Native copy faults
retain their original typed cause and phase. Lifetime counters saturate and do
not reset on invalidation. No hot-path file logging is added by this cache.

Invalidation changes context and clears the cache. A matching-context texture
from another renderer is refused; an old-generation completion is discarded
before accessing its retired device. Stored pixels have separate storage, so
later reuse of the original render target cannot silently alter a cached frame.

## Executed controls

- Exact fractional keys, negative-zero normalization, all four working formats,
  changed context components, invalid time/layout and bounded capacity
- Native UNORM8/Float16 pixels copied independently from an overwritten source,
  including half-float HDR/negative values and alpha
- LRU, byte quota, disabled/recovery paths, layout mismatch, foreign renderer
  rejection and normal late-device completion after invalidation
- Actual shared-budget refusal, recovery and a real encoded GPU consumer whose
  evicted allocation remains admitted until queue retirement
- An explicit manual four-precision checkpoint for optional adapter features;
  it must be executed with `--ignored --exact`, not counted from an ignored row

The source-bound full motion-render run passed **868 tests,
zero failures, two manual/environment exclusions**. Four focused controls overlap
that full run. The manual all-four-precision control additionally ran explicitly:
one passed, zero failed, zero ignored. The external-original-image probe was not
run and remains excluded. Host ai-depth checking and provenance passed. The
source stayed unchanged throughout. No prior assertion was changed.

The separate native producer has since supplied authoritative frozen-byte key
construction; its own acceptance and remaining integration are recorded in
`native-prerender-producer-current.md`. A future native upload increment requires
its own source-bound controls; this primitive receipt does not accept it.

## Remaining #82 delivery

The cache is not yet consumed by Session foreground preview. Immutable source
preparation, authoritative hashing and an actual independent native producer
now have their separate checkpoint. Scheduler-driven job ownership, foreground
upload and consumption, cancellation/installation wiring, cold/warm hit
measurements, paired foreground P95 and optional disposable disk storage remain.
No claim of foreground priority or preemption follows from this cache alone.
Android and network rendering remain outside the maintained local delivery.
