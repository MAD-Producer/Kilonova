# Readback cost observations

Issue #60 distinguishes image readback from timestamp readback and separates
requests, encoding, submission, mapping and returned payload. These observations
do not change rendering, memory admission, error propagation or recovery policy.
They allocate no observation keys or log records on healthy frames.

`Renderer::image_readback_info()` reports bounded cumulative counters over one
renderer lifetime. Creating a capture target does not count as a readback. A
`read_target` call publishes its request and in-flight count before health checks
or GPU work. Successful submission records the RGBA payload and addressed buffer
span. A successful mapped range records the actual mapped length. Only a
successfully returned image increments returned payload/completed calls. An
ordinary error or unwinding exit increments failed calls; a nonreturning call
remains in flight. The original failure remains unchanged.

For a 3 x 2 RGBA8 image, one readback has 24 payload bytes, an addressed buffer
span of 268 bytes and a mapped buffer length of 512 bytes. Row gaps and trailing
padding are distinct from returned pixel data. These are application byte
observations, not measured PCIe/bus traffic or driver residency.

`GpuTimer::readback_info()` reports counters over one timer lifetime. Encoded
query resolves and resolve-to-mapping copies each record their actual 32-byte
word length. Discarded command encoding still counts as encoding; it does not
become a map or completion. Map requests and their actual callback outcomes are
counted separately. Mapped lengths include invalid/zero timestamp words, while
valid/invalid sample attempts remain distinct. A callback without a recorder
sequence still consumed readback work. Calling collect again does not count a
completed slot twice. A new timer, including profiling restart, resets its own
counters. Unsupported timestamp hardware stays absent, not fabricated zero data.
Starting profiling replaces the timer; its counters restart. The renderer is
retained, so image-readback lifetime counters do not restart with recording.

`mapFailures` counts actual `Err` callbacks, including explicit cancellation.
A cancelled request does not by itself emit a fault record or imply a GPU fault.
The callback regression cancels a real pending mapping with
[`Buffer::unmap`](https://docs.rs/wgpu/30.0.1/wgpu/struct.Buffer.html#method.unmap),
polls the actual callback, checks that collection does not count it twice, then
successfully maps again while retaining the lifetime error count. Renderer
health remains valid. No fabricated callback result is used.

Successful normal and exposure frame resource records carry optional image and
timestamp snapshots. Missing renderer/timer observations remain null. Explicit
`resource_memory` separates preview image, preview timestamp and editor image
counters. Preview state and final profiling metadata expose their respective
scopes as well. Image captures outside a recording interval remain part of the
renderer lifetime; first-sample baselines cannot be treated as exclusive frame
costs. Timestamp counts are not inferred from associated frame sample counts.

Each counter saturates at `u64::MAX`; `countersSaturated` makes the resulting loss
of exact counting explicit. Counters are individually atomic and snapshots are
not atomic across fields. Compare only known observations from the same owner
lifetime and retain unknown intervals after missing/reset/saturated counters.
No complete readback coverage is claimed: external decoders, raw foreign queue
operations, other renderer owners and physical transfers remain outside these
two instrumented APIs. Android validation is outside the confirmed desktop scope.

The native regressions use real padded image pixels, actual device destruction,
cancelled mapping callbacks and real timestamp resolve/map operations with
unwritten words. Separate counter
protocol tests cover in-flight publication and saturation. Source or tests alone
are not acceptance; execution receipts are required before reporting completion.
