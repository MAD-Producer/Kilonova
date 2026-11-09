# One retained host-remap service: original sustained observation

Issue #75 remains incomplete. The first declared one-service observation at
`27906e8245acc20643fc3bf80d6e2b9142bd8aab` completed all25 cohorts and800 requests
in1,835.0299092 seconds. Every complete Float32 output matched its sealed cold
reference byte for byte. Thermal signals and unknown device allocation prevent
this from becoming a production qualification.

## Fixed protocol and actual execution

The [pre-run protocol](depth75-sustained-host-remap-protocol.md) was sealed before
execution. Its publication status describes that pre-run point in time; this
document records the subsequent result. The original minimum1,800 seconds,
maximum64 complete cohorts, fixed32 input order and60-second request wait stayed
unchanged. There was no retry, service recreation, warmup deletion, hidden cache
reuse, sleep padding or change to an assertion. Repeated inputs are repeated
load observations, not800 independent domain-quality samples.

The explicit `wgsl-retained-rust-remap-sustained` driver used one measured Jobs
worker and retained native GPU PID38372 throughout. Actual request IDs1–800,
source/model/geometry, execution identities, split/order, complete outputs and
cumulative counts were checked. Each cohort drained its queue and in-flight
state; no failed, canceled, cache-hit, cache-miss or restored count accumulated.
After termination the owned scratch directory was empty. Final owned-job
accounting contained only the observer; the driver and service had exited.

Original controller session71816 exited0, terminal chunk`a54973`; its tool
result was consumed before source editing resumed. HEAD and all tracked source
hashes stayed unchanged throughout. Model, input plan, installed runtime,
native-worker and driver hashes are retained. No model/runtime acquisition,
foreign-process inspection, environment workaround or resource limit was used.

## Complete cost, memory and sensor record

All800 observations are retained, including startup and every slower row.
Quantiles below use nearest rank over the complete ordered population; phase
quantiles are separate measurements and must not be added together.

| Recorded duration | Median | P95 | Maximum |
| --- | ---: | ---: | ---: |
| Worker total | 2.1986208s | 2.5892095s | 13.641116s |
| Identity resolution | 0.322548s | 0.4093024s | 11.8773938s |
| Preprocess | 0.5785774s | 0.7015322s | 2.5826176s |
| Inference phase | 0.4813266s | 0.5618413s | 0.6416497s |
| Rust postprocess | 0.0716907s | 0.0929764s | 0.125013s |
| Host transport | 0.7229273s | 0.8638263s | 1.2021786s |

The independently controlled Windows-owned-job method recorded1,522,769,920
bytes peak aggregate committed memory. It covers this observer, its sensor
children and all workload descendants; no overlapping process peaks were added.
The kernel retained that peak after the children exited. This is not RSS,
physical residency or device allocation. Limit flags remained zero. The fixed
managed GPU buffer capacity remained227,968,156 bytes per inference; a complete
whole-device allocation peak remains unknown.

All1,721 whole-device sensor readings, raw output and flags remain available.
Temperature began at63°C, ended at70°C and reached90°C. Software thermal slowdown
was Active in16 readings; hardware thermal slowdown was Active in one of those
same readings. No sample was discarded. The largest gap between the end of one
query and the next query's start was1.032 seconds. Sensor memory usage reached
3,127MiB; finite samples of a whole device are not a continuous per-process
allocation peak. Unrelated activity can affect whole-device readings, so these
signals do not establish exclusive workload causation or thermal stability.

The default logger reported INFO and retained exactly800 `depth_infer` starts,
request IDs1–800, plus phase starts. Successful driver stdout and stderr were
both empty. Existing bounded Vulkan loader diagnostics remain in the file;
they are not reclassified as proof of a native fault's root cause.

## Evidence and remaining limits

The [archive](data/depth75-sustained-host-remap-v1.zip) and
[manifest](data/depth75-sustained-host-remap-v1.json) contain43 verified members:
the original controller/protocol, source hashes and selected source snapshots,
complete sensor readings, original file/console logs, all25 cohort measurement
reports, output identities, original terminal consumption and collection code.
Archive size856,128 bytes; SHA-256
`95e1a4499c2cdff9a18ad2034e0649032cea0ecde9f726484b1618e2e47a5ada`.
Every member passed CRC, byte-length and SHA-256 readback. Full raw outputs
remain local and were independently rehashed by the collector; no photographs,
learned tensors, model weights, runtimes or executables are redistributed.

This establishes one-service repeat-load completion and exact output stability
on the measured machine. It does not establish realtime performance, absence
of thermal signals, independent application accuracy, complete device memory,
accepted cross-quality video or an automatic fast/standard/high binding. The
[original failed full regression](depth75-host-remap-full-regression-current.md)
and previous Python/native/alignment failures remain failed. The separate
[terminal CI](depth75-ci-dc17b007-current.md) covers its exact earlier source;
neither record substitutes for the other's unmet acceptance.
