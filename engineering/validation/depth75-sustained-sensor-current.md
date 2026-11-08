# Finite learned-depth load and GPU sensor observation (#75)

Status: an actual eight-request Vulkan WGSL workload completed at `d68a9d65`,
with every source hash unchanged. This records finite sampled device behavior;
it does not qualify long-run thermal stability, complete memory or automatic
tier selection. No Android work is included.

The source differs from the previously qualified actual-video source
`8fdb718b` only in validation documents and evidence. All input/model/program,
Python runtime, probe and native-worker executable hashes matched the previous
qualified identities before the workload. Original session 71826 was consumed
with exit 0 before any later heavy work began.

## Workload and observation protocol

The existing actual probe driver made eight sequential uncached requests on
the same authored 12x8 RGB input, using the Small 518 learned model and its
original letterbox procedure. Each request retained resolution, complete worker
wall time and actual execution identity. No cold, warm or slow observation was
removed. The original per-request driver wait and all admission checks remained
unchanged. This workload includes runtime startup and cleanup, as the actual
backend does; it is not a persistent warm-session benchmark.

Documented `nvidia-smi` counters were read before, during and after the complete
workload, with a requested one-second gap between completed queries. The
controller retained all 141 readings, bounded to at most 512. The maximum actual
unobserved gap between queries was 1.047 seconds; query duration is also retained.
The environment record binds Windows, RTX 4070 Laptop GPU identity and NVIDIA
driver 591.86. The measurement-protocol digest is
`ae2e0294dd4cf3fd0081eb9abb30260aeb579df996e22e789de1481ab5c5102f`.

The sensor covers the entire physical GPU, including unrelated applications.
No other application, overlay, process or machine setting was stopped or changed.
Temperature, utilization, graphics/memory clocks, power and device-memory values
are retained where the driver reports them; unavailable fields remain unavailable.
A sensor observation does not attribute heat, power or memory to this workload.

## Actual results

- All eight attempts completed, with zero failure, cancellation, cache hit or
  spill restore; total workload wall time was 151.875 seconds
- All worker totals, in request order, were 21751.103, 19596.411, 18367.748,
  17667.707, 18290.912, 19676.185, 16854.063 and 18916.153 milliseconds
- Sampled GPU temperature ranged from 70 to 77 degrees Celsius, starting at 70
  and ending at 72; neither sampled hardware nor software thermal slowdown flag
  was active
- The greatest observed whole-device memory use was 3,457 MiB; this includes
  unrelated device use and is a sampled maximum, not a proven allocation peak
- Each successful inference separately retained the already validated
  227,968,156-byte managed buffer capacity; this remains a different quantity
  from physical-device memory or host allocations
- Default Info starts were verified before resolution and inference for every
  request; the existing loader errors remain in the raw logs and were not hidden

The [archive](data/depth75-sustained-sensor-v1.zip) and
[manifest](data/depth75-sustained-sensor-v1.json) preserve the complete bounded
sensor series, controller, raw driver logs, measurement and original terminal
receipt. All nine members were read back and verified: 309,282 bytes, SHA-256
`b964040fbe7adc7972011e65e6420bc951c41352c8113c5ebd5edef551df50a5`.
Independent reduction reproduced all eight times and the sensor summary.
No weights, model plans, executables, video, RGB or depth tensors are included.

## Remaining limits

A finite 152-second run and sampled temperature maximum do not establish
long-run stability or a continuous maximum. The record contains no CPU sensor,
whole-process host peak, independent-reference quality, transition jump or
process-attributed energy measurement. It cannot alone satisfy every external
qualification section required by the capability policy. Actual independent
domain quality, complete memory evidence, longer sustained acceptance and
continuous-video transition validation remain open. The product has not been
switched to automatic tier selection, and issue 75 remains incomplete.
