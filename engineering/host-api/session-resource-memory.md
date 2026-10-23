# Explicit Session resource memory observation

Issue: #60. Send `{"op":"resource_memory"}` through the existing plugin JSON
bridge, or use `motion_host::ops::preview::resource_memory(session_id)`.
`Session::resource_memory_ledger()` provides the same read for native callers.
This query does not initialize graphics, submit work, modify the project, change
history or run during ordinary preview snapshots/frame sampling. The existing
`resource_ledger` operation continues to return its texture-only contract.

The version 1 response separates three domains:

- `textures`: the existing deduplicated logical GPU texture ledger, including
  its original coverage and pending-submission limitations
- `cpu`: observed owned Vec payload and backing-storage capacity for active
  video frame caches, preview video proxies, Session audio output scratch and
  the serialized composition bundle, cached font glyph coverage, mixer scratch
  and deduplicated shared resampling tables, plus package archives, expanded files
  and package-owned compiled shader text deduplicated across retained builders
- `external`: driver residency, decoder allocation and process residency remain
  null; no CPU or texture subtotal is presented as total process RAM/physical VRAM

An active decoded frame can be referenced by more than one stream cache.
`videoDecodedCache` deduplicates the actual Arc frame identity, not its source,
PTS or pixel contents. Equal-looking independent frames remain separate.
`payloadBytes` uses Vec lengths; `capacityBytes` uses actual Vec capacities.
Both YUV planes count without materializing an RGBA copy. Frame identifiers
exist only within one snapshot; raw pointers never enter the response. Temporary
Arc clones keep those allocations alive while the query deduplicates them and
are released before returning. Cache policy and its original payload counters
remain unchanged.

The query uses `try_lock` for active video caches. Busy streams are listed with
null byte values and `allActiveStreamCachesObserved=false`; the known subtotal
is partial, not evidence that those streams own zero bytes. A later query can
observe them after release. Poisoned state returns an error with object, source
and phase, preserving the existing caller's error reporting. No logging occurs
while a cache guard is held. Streams are sampled sequentially, not atomically.

Preview proxies own their pixel vectors. Their weak references to original
decoded frames do not add frame ownership or duplicate decoded-cache bytes.
`audioOutputScratch` counts f32 storage in bytes; clearing its payload leaves
capacity until the Vec is replaced or shrunk. `compositionBundle` follows the
same rule for its byte vector.

Every domain publishes its exclusions. In-flight/retiring decoder output,
external frame readers, image decode worker/completion pixels, original-source
preview inputs, font parser internals, escaped glyphs/rasters, mixer project and
container storage, independent effect-program shader copies, package metadata,
external package owners, models/jobs, inference and
allocator/container overhead remain outside this increment. Their capacity is
unknown, not zero. The complete response is not an atomic frame peak and keeps
`completeCoverage=false`.

Acceptance requires actual cache contents, alias/distinct-frame checks, real
Vec spare capacity, YUV planes, eviction/release, controlled lock contention,
poisoning, public bridge and unchanged project/history tests. These CPU tests
are not file-decode, GPU performance or Android-device evidence.
