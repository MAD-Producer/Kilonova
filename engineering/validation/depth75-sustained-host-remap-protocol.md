# Retained host-remap sustained protocol

Status: implementation and all four example controls passed. The first real
one-service sustained observation has not yet run. This protocol is fixed
before that observation and is not a qualification result.

The explicit `wgsl-retained-rust-remap-sustained` example entry will use one
measured Jobs worker and one retained GPU service for the entire invocation.
It repeats the original sealed fixed32 input pairs in their original order,
with no warmup deletion, cached inference, skipped request or retry. Source
PTS and bytes remain original. Repeated frames are repeated-load observations,
not new independent domain-quality samples. Every actual request gets its own
monotonically increasing worker ID.

Stop only after a complete32 cohort reaches at least1,800 seconds. At most64
cohorts (2,048 requests) are allowed. Reaching the cohort cap before the minimum
duration fails rather than extending the cap or padding the run with sleeps.
The request wait remains60 seconds. The driver stops on its first workload
failure and preserves completed and partial cohort files. An observer failure
marks the attempt failed without restarting the driver; the observer waits for
the already launched driver to reach its own terminal state and retains any
later files as part of that failed attempt. No schedule value will be adjusted
in response to its first outcome.

Per-cohort Profile capacity remains32, below the product's original64 bound.
Every cohort's full original measurement rows, outputs and exact output hashes
are retained. No observation eviction or reservoir sampling substitutes for
the complete reports. All outputs will be compared with the sealed historical
cold output for that same index. All requests must identify the same owned
native GPU PID and the exact same execution and fixed managed GPU capacity.

The separate Windows-owned-job probe established actual kernel-maintained
aggregate committed-memory accounting and descendant coverage for its two
owned16MiB allocations. It recorded three simultaneous members,61,079,552 bytes
job peak versus24,354,816 bytes largest process peak, and retained the job peak
after both children exited. Its own limit flags stayed zero. The metric is not
RSS or device allocation. In the real run it includes observer/sensor overhead
and all workload descendants; no foreign process joins the new owned job. GPU sensor
coverage will retain all whole-device readings, foreign activity limitations,
temperature maxima and thermal flags. Any unavailable counter stays unknown.
Neither finite sensor samples nor this proposal certify device memory or heat.

The original full workspace82855 became terminal and its actual tool result was
consumed before source editing resumed. It remains failed with its original
four failed targets. The first actual long-run command will bind current source,
example/native-worker binaries, installed runtime/model/program, input plan,
sensor method and this protocol. No acquisition, cache cleanup, overlay change,
foreign-process inspection or shutdown is part of this protocol.
