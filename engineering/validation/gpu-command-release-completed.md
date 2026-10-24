# Native encoder baseline: completed Release measurement

Issue #60, maintained desktop scope. Source `ff52fbd2abd370b1d72fa6c86231cf8f4d24fa21` completed six actual
Release workloads: **408 raw frames / 360 measured frames**, with 8 cold/warmup
and 60 measured frames per case. The fresh source-isolated target used the
normal repository Release profile, without profile overrides or cross-source
workspace-binary reuse. Both actual collection and offline replay exited zero.

Adapter: **NVIDIA GeForce RTX 4070 Laptop GPU**, API **Vulkan**, driver
**NVIDIA 591.86**. The development desktop was
uncontrolled; independent background tasks, power and temperature were not
certified. No comparative optimization or throughput claim is made.

## Actual samples

All projects pin 1280 x 720, 30 fps, full-resolution quality and a 256 MiB
source-texture allowance. Static samples frame zero; other cases sample authored
frames 0 through 67. Original project JSON pins effects, parameters and keys.
First-frame/cold, seven warmup and 60 steady records remain separate. Cold
means a fresh renderer, not a purged OS/driver shader cache. Percentiles below
use the original 60 measured records and nearest-rank selection, in microseconds.

| Case | Native command records | CPU prepare p95 | Root GPU p95 | Main encoder interval p95 |
| --- | ---: | ---: | ---: | ---: |
| static | 68 | 103.000 | 39.936 | 62.464 |
| animated | 68 | 93.000 | 29.696 | 245.760 |
| effect-chain | 68 | 914.000 | 2785.280 | 3326.976 |
| crossing-3d | 68 | 100.000 | 28.672 | 666.624 |
| nested-matte | 136 | 1244.000 | 37.888 | 267.264 |
| four-4k-yuv | 340 | 100.000 | 183.296 | 806.912 |

Total actual command records: **748**.
Each retains original query words, the native queue timestamp period, epoch/id
and exact offline caller/allocation-window context. Main encoder IDs are exact
scalar links, not joins on floating source frame or a recent-record ring.
Offline request IDs are explicitly case-local sample ordinals, not Engine
worker requests. No active, missing, omitted, quarantined or failed diagnostic
samples are accepted in these completed recording windows.

Static vector/MSAA preparation precedes root timing inside the main encoder;
nested composition and four per-frame 4K YUV conversion submissions retain
separate command intervals. Root and encoder intervals overlap and must never
be summed into exclusive GPU busy time. Encoder intervals include marker/backend
overhead. Queue writes before an encoder are outside its timestamp interval.
Observed command wall elapsed includes native completion/mapping; it is not
exclusive CPU preparation.

Per-frame diagnostic readback is 16 bytes per command, recorded separately
from the existing 32-byte root/presentation timestamp readback. Image readback
is zero inside timed samples; one final PNG is captured after trace stop.
Independent Pillow decoding checked all six dimensions and original native
RGBA hashes. This proves byte consistency, not an independent intended-scene
visual oracle. Generated YUV is not file decoding: a single reusable CPU fixture
is counted once while four actual uploads/conversions remain four operations.

## Original evidence

The [raw Release archive](data/gpu-command-release-ff52fbd2.zip) contains
44 entries. Its 43
payload entries were independently verified by size and SHA-256, in addition
to ZIP CRC. Original build/native channels, start/exit receipts, provenance,
binary identity, six authored projects, reports/PNGs, raw samples and summary
are retained. No executable or third-party media is packaged.

Archive SHA-256: `0db85a2e00bc00fa12009216a3318414cfec429ae7e0cefdecdc706c3485ad8f`.
The [acceptance manifest](gpu-command-release-acceptance.json) binds its exact
source, original acceptance and independent PNG proof. The matching
[correctness archive](gpu-command-observations-completed.md) contains the
source bundle/patch, all tracked input hashes and retained historical failures.

To replay, extract the archive and use the collector at the recorded source:

```console
python tools/collect_desktop_performance.py --output <extracted>/measurement --validate-only
```

Historical schema-1/2 measurements remain attributed to their original sources;
no missing command observations are invented for them. This new study is
headless serialized latency with deliberate native completion waits, not
window playback, physical display deadlines, exclusive GPU utilization or
physical memory exhaustion. Actual encoded-video/application-budget studies
retain their separate original sources. Android validation is excluded.
Logical capacity, CPU payload/capacity and unknown external allocations remain
distinct. No full GPU/process-memory coverage or measured acceleration is claimed.

Final-head CI remains necessary for delivery. No UI, dependency, asset or
interaction change is introduced by the study or these completion records.
