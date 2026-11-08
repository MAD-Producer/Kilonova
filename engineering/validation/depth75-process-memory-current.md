# Owned inference-process memory observations (#75)

Status: implementation and validation in progress. This does not complete #75.

The optional ONNX worker queries its own OS high-water marks once, after output
preparation. The observation includes runtime initialization, model loading and
inference up to that query. It does not include subsequent report serialization,
the host, the identity-resolution child, other children, or physical GPU memory.
The resident and private-commit peaks may occur at different instants; they must
not be added. Working-set bytes are not a weight/activation allocation split.

Windows uses `GetProcessMemoryInfo` on `GetCurrentProcess` with the independently
declared `PROCESS_MEMORY_COUNTERS_EX` layout. `PeakWorkingSetSize` is resident
bytes; `PeakPagefileUsage` is peak private commit, not physical pagefile traffic.
Linux uses `getrusage(RUSAGE_SELF).ru_maxrss` in KiB, converted to bytes. Darwin
uses the current XNU `resident_size_max` value in bytes, rather than the stale
unit in the archived manual. Unix private commit remains unknown (`null`).

No process enumeration, optional package, periodic sampler, device inspection or
new dependency is involved. Unsupported platforms and query failures produce a
bounded `unavailable` cause, not a fabricated zero and not an inference failure.
The report parser rejects malformed counters and method-specific counter inconsistencies.
No memory budget is certified by this partial observation.

`Costs`, measured `Jobs`, retained `Profile` observations and the three real
capability/reference/video probes expose the observation. It is not part of the
execution identity: measurements varying between runs cannot invalidate an
otherwise identical runtime. The embedded worker remains part of the existing
execution-plan hash, so changing this procedure changes that attested identity.
WGSL currently reports no inference-child observation; this is unknown, not zero.

Validation must include the real owned child on each maintained desktop OS,
normal and optimized Python, touched/released allocations, bounded unavailable
causes, Rust report validation/retention, complete relevant native targets, and
an actual learned inference through the rebuilt probe. Mocked counter metadata
only verifies retention and cannot substitute for actual OS or model evidence.
Results and exact source identities will be recorded after the original runs
finish. Whole-host/device peaks, allocation attribution, sustained thermal
behavior and qualified automatic selection remain separate pending acceptance.

Primary API references and pinned Darwin source are recorded in ATTRIBUTION.md.
