# Sustained load and current affine-alignment observations (#75)

Status: finite observations completed at `ad799f2f`; #75 remains open.
Complete current maintained-workspace regression and current-source CI remain
separate pending requirements. Android is retired and excluded.

## Original sustained workload

The precommitted [protocol](depth75-sustained-reference-protocol.md) fixed a
1,800-second minimum, complete 32-frame cohorts, at most six cohorts, unchanged
60-second request waits and a five-second maximum sampling gap. The actual
WGSL518 route completed four full cohorts: **128 uncached requests over
1,887.250 seconds**. Original session 65993 terminated and was consumed with
exit 0. Tracked source hashes and bound executable/runtime/model/program/input
identities remained unchanged. No failed request, warm-up or slow sample was
removed and no inference was retried.

Every request retained valid host and all five direct-child OS observations:
768 observations in total. The host-plus-largest-sequential-child observed
resident sum ranged from 183,771,136 to 186,499,072 bytes. This is a combination
of queried high-water marks before serialization/exit, not a final-lifetime
upper bound, simultaneous physical usage or complete process/device accounting.
Resident and private-commit counters remain separate. The largest prepare
private-commit observation was 814,845,952 bytes; native inference reached
655,167,488 bytes. Managed GPU buffer capacity remained 227,968,156 bytes.

| Complete worker time | Seconds |
| --- | ---: |
| Minimum | 11.9390487 |
| Nearest-rank p50 | 14.1057411 |
| Nearest-rank p95 | 18.5753275 |
| Maximum | 27.5304350 |

These are cold product-route worker costs, including resolution, preprocessing,
inference, remapping and host transport. Source-file load/output-write timings
are retained separately. They do not establish interactive realtime operation.

## Whole-device observations and thermal limits

All 1,782 original sensor queries completed. No readings were evicted. The
controller's maximum next-start-minus-previous-end idle gap was 1.047 seconds.
An independent audit also included query duration: the largest gap between
completed readings was 1.234 seconds. Both are below the predeclared five-second
coverage limit. Query duration itself reached 0.234 seconds.

Sampled GPU temperature ranged from 66 to **87 degrees Celsius**; p50/p95 were
69/71 degrees. Software thermal-slowdown flags were Active at three readings;
hardware thermal-slowdown flags were Not Active at all 1,782 readings. The three
Active readings separately reported 69–71 degrees, a 2,580 MHz graphics clock
and 0–7 percent utilization. Those samples do not establish the cause, exact
duration or model-specific latency impact of a slowdown. The 87-degree maximum
is retained. Neither these flags nor that maximum is waived as an outlier.

These are sampled whole-GPU counters, including unrelated activity. Sampled
whole-device memory reached 2,297 MiB. Power-limit data was unavailable and
remains unknown. No applications were stopped, device policy changed or foreign
process inspected. This record **does not qualify thermal stability**, physical
allocation peaks, model-attributed power/temperature or a complete memory budget.

## Fixed independent sensor residuals

One positive affine inverse-depth calibration used all valid sensor pixels
from the first eight frames of cohort zero only. Its fixed coefficients were
applied to held-out frames 8–31 in every cohort. There was no later refit,
clipping, residual trimming or per-frame normalization. Each 24-frame held-out
cohort retained 5,637,354 valid and 1,735,446 missing sensor pixels, with RMS
0.10997563568210106 inverse metres and maximum 1.1606679537404099 inverse metres.

All 96 repeated-source output digests in cohorts 1–3 matched their corresponding
cohort-zero outputs. This explains the equal repeated-source residuals; it does
not make repeated frames independent new domain-quality observations. The
original attributed TUM source, registered sensor units and input hashes remain
those of the earlier sensor-reference protocol. No dataset pixels or learned
arrays are redistributed.

## Actual current-product alignment over consecutive source coordinates

A separately frozen native observer called the current Rust `Alignment::fit`
and bound successful proofs to the exact reconstructed resource instances. It
used the existing four sealed learned-output cohorts at all 32 original ordered
source coordinates, spanning 1.031993 seconds. It checked each original plane's
size, digest, finite values, exact prepared RGB/source/geometry and model metadata.
It did not rerun inference, decode a new video or invoke Session switching.

CPU518 was the declared reference. Explicit research ceilings were minimum
1,024 fitting and 10,000 held-out pairs, full paired coverage, reference standard
deviation at least 1e-8, scale in [0.1, 10], normalized RMS at most 0.01 and maximum
at most 0.05. Normalization uses the product fit-reference standard deviation.
These are research ceilings, not independently qualified application limits.

| Candidate | Inspected | Accepted under these ceilings | Rejected |
| --- | ---: | ---: | ---: |
| CPU266 | 32 | 0 | 32 |
| CPU392 | 32 | 0 | 32 |
| Same-weight WGSL518 | 32 | 32 | 0 |

The first CPU266 fit-population RMS/maximum were 0.26923565/2.10410754; CPU392
reported 0.21276445/1.81956057. Every original typed refusal is retained. Rejected
fits do not produce held-out statistics; missing statistics remain absent.
No ceiling was changed to obtain acceptance. The separate same-weight backend
control does not qualify a cross-resolution or cross-quality switch.

For the 31 adjacent accepted WGSL518 pairs, the observer also retained change in
aligned-candidate-minus-reference residual on the same source grid. Rejected
frames break adjacency. This is not scene-flow correspondence, an accepted jump
budget, independent sensor truth or an actual interactive Session transition.

The actual native build and all three observations completed. Separately authored
corrupt-plane-digest and changed-prepared-input controls each exited with their
original error before alignment, with default-Info starts, Error in file and
stderr, empty stdout and no result. Original session 21284 was consumed with
exit 0; no existing test, assertion or product behavior changed. The observer's
registry packages were checked against the existing locked versions/checksums.

## Evidence and remaining acceptance

The [manifest](data/depth75-sustained-alignment-observations-v1.json) and
[archive](data/depth75-sustained-alignment-observations-v1.zip) retain 99 members:
original protocols/controllers, raw logs, commands, all observations/refusals,
sensor readings, fixed-fit reduction, independent audit, original failure
controls and selected source snapshots. All members passed CRC and SHA-256
readback verification. The archive is 1,553,945 bytes, SHA-256
`8f0af080f2d124ed5473593ac6ec2812ef797052e29cf80ad46b77e21f0e2e74`.
Model/runtime binaries, executables, photographs and RGB/sensor/learned arrays
are excluded. Selected snapshots plus all-source hashes do not constitute an
archive of the entire repository source tree.

The archive also retains three-platform test logs for earlier source b65a016f,
including all nine exact named memory controls. Its original collector's wrong
test-module-name failure remains retained alongside the corrected log-only
continuation. Tests and assertions were unchanged. That earlier source's test
pass does not qualify later native/phase-memory implementations or the entire
then-running workflow.

Source ad799f2f is aligned to freshly fetched main ac65d279. Its
[current-source desktop CI](depth75-platform-memory-current.md) completed with
all eight applicable jobs successful and 15 exact named memory-control records.
Its [complete maintained-workspace command](depth75-full-regression-current.md)
finished with Cargo exit 101: 5,274 passes, two failures and 78 original ignores
in completed summaries; two additional native targets exited without normal
summaries. All 449 expected invocations were observed, but this is not a complete
regression pass. Original session 87729 was consumed with controller exit 1 and
source unchanged; no failure was retried. Physical and complete lifetime memory
qualification, application/domain quality admission, accepted continuous-video
quality transitions and automatic tier selection remain open. The real thermal flag observations and all cross-resolution
refusals remain part of acceptance; they are not converted into successful
qualification. #88 still retains its original photographed tracking failures.
