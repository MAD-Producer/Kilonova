# Native composition upload checkpoint (#82)

Status: implementation authored; compilation and actual controls are pending.
This adds a foreground destination API, not an Engine runtime or preview cache
consumer. The prerender capability remains disabled. Android is excluded.

`CompositionFrameCache::prepare_native` reserves a real texture on the receiving
renderer before background work is dispatched. The destination shares the live
allocation budget and the cache's entry/byte quota. Only one destination may be
active per cache. Existing GPU-copy insertion also accounts for that reservation.
An unavailable slot/quota is a normal false/None outcome, not a fake admission.

`install_native` accepts the reserved destination plus native CPU pixels. It
checks cache identity/epoch, document/device/content/output context, renderer
owner, the linear-premultiplied semantic contract, exact physical storage,
dimensions and payload length before queue IO. It preserves encoded Unorm8 bytes
and exact higher-precision values without conversion. Stale completion is a
normal false result, checked before touching an obsolete renderer. Invalid
layout is an error and cannot replace valid cached pixels.

Invalidation, explicit clear and changed limits invalidate pending destinations.
An abandoned destination releases its actual admission when its owner drops it.
A retained stale destination or evicted cached handle remains charged to the
shared budget; cache resident/pending counters do not claim driver residency or
release while another owner/GPU submission remains. Upload writes enroll the
destination's allocation before queue IO, then retain it through actual queue
retirement. No texture from the background device is imported.

Four new ordinary GPU controls cover real admission and combined cache limits,
malformed layout/semantic refusal with preserved pixels and recovery, cache/
device/policy generation changes, and shared-budget refusal/recovery/retirement.
The explicit manual control checks all four native storage formats byte for
byte, including encoded Unorm8 and floating HDR/negative/alpha values. Existing
cache assertions are unchanged; three test helpers only gain sibling visibility.
These controls are not counted as passing until the source-bound run completes.

The independent native producer's preceding accepted source is recorded in
`native-prerender-producer-current.md`. This new upload code requires its own
regression and cannot inherit that acceptance. Actual bounded job ownership,
CPU completion quotas, Engine cancellation/observations, Session foreground
consumption, profile invalidation and paired foreground P95 remain pending.
No UI code, layout, style, hit tolerance, dependency or asset is changed.
