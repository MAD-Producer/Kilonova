# Sustained ordered reference workload (#75)

Status: protocol frozen before execution; no result or qualification claim yet.
Desktop only. This extends the earlier finite eight-request sensor observation.

The workload repeatedly runs the existing full 32-frame sensor-reference cohort
through the actual WGSL518 backend. The original input order, RGB hashes,
640x480 geometry, model/weights, program, source staging, resolution, transform,
inference, remap and output ownership checks stay intact. Every request uses
the current cold child-process procedure; this is not a persistent warm-engine
benchmark or an isolated shader stress test. No request, warm-up, slow sample
or loop boundary is removed. The discontinuous cohort loop is explicit and is
not a continuous-video transition qualification.

The predeclared minimum active workload wall time is 1,800 seconds. A complete
32-request cohort is always finished. Start another complete cohort only while
below 1,800 seconds, with at most six cohorts (192 requests). A failed request
ends the study without retry. If six complete cohorts finish below the minimum,
the minimum-duration gate remains failed; no seventh cohort may be added to
make it pass. The existing per-request 60-second probe wait is unchanged and is
not an interactive latency budget. All worker times and child-stage memory
observations are retained, not just the fastest or warmest subset.

The controller reads the same documented nvidia-smi fields as the original
study, at a requested one-second gap after each query, before, throughout and
after the actual workload. At most 4,500 samples are retained; none is evicted.
All raw query outputs, elapsed query durations and missing/unavailable fields
remain observable. The sensor targets the single reported NVIDIA device and
checks its identity against the actual execution provider. UUID and driver
must remain stable. A maximum observation gap above five seconds is reported
as a failed sampling-coverage gate, not hidden by interpolation.

Temperature, power, utilization, clocks, thermal-slowdown flags and memory
describe the whole physical GPU, including unrelated activity. The controller
does not stop applications, alter clocks/power settings, enumerate foreign
processes or attribute device heat/energy/memory to this model. No unavailable
CPU temperature, process descendant or physical allocation peak is invented.
Temperature and clock flags are observations, not a universal hardware safety
threshold or a guarantee beyond this workload and environment.

After the complete load, retain the raw relative depth outputs and all probe
receipts. Use the same independently attributed sensor data and metric code.
Fit one positive affine inverse-depth mapping on the first eight frames of the
first cohort only. Apply that fixed fit to held-out frames 8..31 in every
cohort; no later cohort/frame refit, clipping, trimming or outlier removal.
This measures repeated-source residuals under load, not general domain quality,
new quality tiers, arbitrary-scene acceptance or a transition-jump limit.

Build the actual reference probe and native worker once, bind executable,
runtime, model, program, input and tracked-source hashes, and freeze source
until the original controller and child handle terminate and are consumed.
Use one owned heavy workload. Build/model/sensor failures retain original
outputs; a failed sensor observation does not authorize a replacement workload.
The actual child is reaped before ending the controller. No Android, panels,
new dependency/asset acquisition, cleanup or retry is included.

Acceptance of the study requires all planned requests to complete, the minimum
duration, complete ordered outputs, unchanged identities/source and reported
sampling-coverage result. It does not by itself qualify complete memory, thermal
limits chosen by an application, tier policy or the whole issue. Publish the
actual maximum latency/temperature and all observed slowdown flags, including
any failure. Earlier shorter runs and failures keep their original results.
