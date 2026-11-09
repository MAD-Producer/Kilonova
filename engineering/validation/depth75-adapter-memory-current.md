# Inference adapter memory observations

Status: implementation and component controls complete; actual learned-model
telemetry and complete regression are not accepted. Issue #75 remains open.

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
