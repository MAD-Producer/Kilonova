# Controls before the one-service retained host-remap observation

The explicit `wgsl-retained-rust-remap-sustained` reference example keeps one
measured Jobs worker and one retained native GPU service across complete ordered
32-input cohorts. Its [protocol](depth75-sustained-host-remap-protocol.md) fixes
the minimum workload at1,800 seconds, at most64 cohorts/2,048 requests, the
existing60-second request wait and bounded per-cohort profiles. Reaching the
cap early fails; no sleep padding, retry, skipped request or warmup deletion is
permitted. Completed and partial files survive the first workload failure.
Repeated inputs are load observations, not new independent quality samples.

Only this example and its new scheduling module changed. The original fixed32
route, its three tests, product backend, model, UI and panel code are unchanged.
The first complete example-control command passed all four tests with zero
failures, ignores or filtering; its production example build also passed. The
new test covers the1,799/1,800-second boundary, the64-cohort cap, invalid cohort
counts and refusal to accept a short capped run. Original session23477 returned0
and was consumed before editing resumed. Source hashes remained unchanged.

## Independent host-memory counter control

A new observer joined only itself to its own Windows job without setting any
resource limit, breakaway flag or termination policy. Two owned child processes
each allocated16MiB and waited for their parent's release. The control verified
their inherited membership and concurrent presence through the exact handles
returned by its own process creation. It did not open or attach another process.

The observed job peak was61,079,552 bytes, compared with24,354,816 bytes for the
largest individual process peak. The kernel retained that job peak after both
children exited and active membership returned from three to one. Its own limit
flags remained zero. The original control exited0 with empty stdout/stderr and
no cleanup failures. These are actual controlled allocations, not a mocked
counter or the sum of independently observed lifetime peaks.

This metric is committed memory, not resident memory, physical memory or GPU
allocation. The later real workload must record its own counter values and
descendant coverage. Its observer and sensor overhead are included explicitly.
This small control does not qualify the model's host-memory requirement, and
finite GPU sensor samples cannot establish a continuous device-allocation peak.

## Evidence and remaining work

The [archive](data/depth75-sustained-host-controls-v1.zip) and
[manifest](data/depth75-sustained-host-controls-v1.json) retain the original
complete component command/output, source bindings, consumed result, exact two
example sources, independent owned-job control/results and fixed protocol.
Every member passed CRC, byte-length and SHA-256 readback. The archive has
322,531 bytes, SHA-256
`b2550df00736ff969439c270340335f7b280af2d6432ca8170331fe2b2f8933d`.
It contains no model, runtime, executable, RGB or learned tensor.

The first actual sustained model observation is still pending. These controls
cannot replace it, independent domain quality, accepted cross-quality video
transitions or current-source CI. The
[original full regression](depth75-host-remap-full-regression-current.md)
remains failed and is not rerun by this increment. No assertion or scientific
ceiling was changed to obtain acceptance; #75 remains incomplete.
