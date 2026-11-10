# Inference adapter memory observations

Status: implementation, component controls and actual learned-model telemetry
diagnostic complete; production qualification remains pending. Issue #75 stays open.

The native depth worker now binds one read-only memory observer to its actual
adapter. Cold, batch and retained requests record before/after observations,
including their sample age, backend and inference process ID. Successful reports
carry these typed observations into host costs and the reference sampler.
Queries reuse the renderer's one-second cache and failure suppression.

These are advisory native pool observations, not physical capacity, per-request
or process peaks. Cached before/after values may represent the same query.
Unsupported queries retain their explicit reason. Usage exceeding a changing
budget remains valid evidence; it does not become an inference refusal or zero.
Managed buffers, process committed peaks and these observations are not added.

The host rejects missing telemetry, wrong process/backend identity, peak or
capacity claims, unsupported schema versions, oversized causes and invalid pool
sets. CPU estimates retain an absent optional observation. The native worker
and host must be rebuilt together; legacy worker receipts are not accepted.

## Source-bound controls

Private raw records reside under `artifacts/pr-queue/`:

- `depth75-adapter-memory-controls-v1`: native check failed because the CPU cost
  constructor lacked its new optional field. The explicit `None` was added.
- `depth75-adapter-memory-controls-v2`: native check passed; complete host library
  reported 293 passed, one failed, one ignored. A separate native identity
  fixture lacked required process/sample metadata. Only that fixture changed.
- `depth75-adapter-memory-controls-v3`: native check passed; complete host library
  again reported 293 passed, one failed, one ignored. Both new receipt controls
  and the corrected identity control passed. The original multiview exposure
  control failed with a native WGSL parser error at byte 5683, line 109: expected
  `)`, found end of input. Its cause remains unresolved. No original assertion,
  ignore, deadline or threshold changed. This full target remains failed.
- `depth75-adapter-memory-controls-v4`: only previously unexecuted commands ran:
  both actual DX12/Vulkan adapter tests passed; native worker build passed;
  all four reference-example tests and its build passed. No host retry occurred.
  Original terminal tool output was lost during context truncation; its handle
  subsequently returned unknown. The terminal receipt, all eight raw logs and
  unchanged source digest were independently verified. This is not represented
  as consumption of an unseen original terminal result.

All four v4 commands ran against base `f1acc1535ddb263084a720a5e085894206420a85`
and tracked diff SHA-256
`b96e7d62e72a681e19c928642cf2381afda1388ef6430b3f4c8307e5e8d0908d`.
Original commands were locked/offline, serial, with no compiler, backend, stack
or environment workaround and no model/runtime/dependency acquisition.

The successful original published-source CI runs #42605523017 (#651) and
#42601997144 (#582) precede these changes. They do not cover the new telemetry
or resolve local failures. Domain quality, cross-resolution consistency, thermal
qualification and complete device peak allocation remain separate pending gates.

## Exact-module diagnostic and actual telemetry

The subsequent [parser diagnostic](depth75-shader-parse-current.md) retains
the original failure and adds module fingerprints. Its complete host-library
command passed with 294 passed, zero failed, one original ignore and no filtering.
This does not establish the cause or repair of the preceding native parser error.

`depth75-adapter-memory-telemetry-diagnostic-v1` then rebuilt both the worker and
reference driver, copied and hashed their executables, and ran all 32 fixed inputs
in their original order with the original 60-second request wait. Every complete
Float32 output matched the sealed cold baseline byte for byte. There was one
native inference PID (54176); all 64 before/after samples belonged to that same
process and reported available Vulkan device-local heap data. Sample ages ranged
from zero to 511 ms. Observed usage ranged from 474,787,840 to 476,884,992 bytes;
this range is not a peak-allocation measurement. Scratch was empty after closure.

Default Info retained all 32 host remap starts, and successful stdout/stderr were
both empty. No retry, output deletion, input reordering or threshold change took
place. The original terminal result was consumed (session 83426, chunk 6fd75d),
and the source remained unchanged throughout. The tested source is base
`fe3a13ab4ed9e06400eb2eb4e8fa19666cd172bf` plus tracked diff SHA-256
`b6a2186132376d1d6726a93d028082c5d2e5e9f0b0e7156bf05e2cd4e180b3df`.

This separately scoped telemetry diagnostic does not run or weaken the original
unmet qualification controller (`depth75-adapter-memory-actual-v1`), which remains
unexecuted. Original domain, cross-resolution, thermal and complete-regression
failures remain. All records, command results, executable hashes, measured outputs
and observation summaries are retained under its private artifact directory.
