# Owned inference-process memory observations (#75)

Status: Windows self-process observation and actual ONNX propagation verified at
`b65a016f`. Current cross-platform validation and broader acceptance remain open.
This does not complete #75.

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
The retained results below cover Windows; other desktop OS checks remain
separate. Whole-host/device peaks, allocation attribution, sustained thermal
behavior and qualified automatic selection remain separate pending acceptance.

Primary API references and pinned Darwin source are recorded in ATTRIBUTION.md.

## Retained results and failures

Complete tool discovery at `b65a016f` ran 149 tests: 148 passed and one original
skip. The three new controls include actual touched/released 64 MiB allocations
in distinct owned Python children, both normal and optimized. The provenance and
inventory guards passed. The first native invocation incorrectly supplied
desktop-media features to a host-only selection; Cargo refused it before any
test ran. Original session 21706 was consumed with exit 1.

The corrected complete host library target ran 276 tests: 275 passed and one
failed. All three new memory format/report controls passed. The failure was
`tracking::project::tests::actual_expression_references_and_scene_hierarchy_include_source_displacement_exactly_once`,
at `project_tests.rs:199`, where expression evaluation reported
`execution_deadline elapsed_us=28733 frame_remaining_at_entry_us=91440`.
Original session 67838 was consumed with exit 1. This original failure remains
failed; no assertion, timeout, budget or production expression code changed.
Its causal relationship to machine load or these changes is not established.

The independently continued, previously unstarted complete capability and
Session targets passed 39 and 32 tests respectively: 71 passes, zero failure,
ignore or filtering. Actual reference/capability/video probes and the WGSL
worker then built. A rebuilt capability probe ran two uncached CPU266 learned
inferences on the same frozen 640x480 reference RGB, with the existing model.
Both distinct inference children reported Windows counters through measured
Jobs and probe JSON:

| Inference PID | Peak resident bytes | Peak private commit bytes |
| --- | ---: | ---: |
| 28120 | 355,684,352 | 1,113,407,488 |
| 5828 | 355,770,368 | 1,113,485,312 |

Default Info starts were present in the actual file log. CLI stdout contained
the intended measurement JSON, without logger records. Original session 32604
was consumed with exit 0; tracked source hashes remained unchanged throughout
each original command. These repeated RGB inputs prove actual metadata
propagation, not independent quality, continuous-video or thermal qualification.

The [manifest](data/depth75-process-memory-v1.json) and
[archive](data/depth75-process-memory-v1.zip) retain original controllers,
commands, failures, successful supplementary outputs, source snapshots and
actual observations. Every member was read back and SHA-256 verified. The
archive is 1,548,349 bytes, SHA-256
`a8c94307902f8fa54321368c699eb7323891b6cd9d6d4781f8ba8001a0a6c9fb`.
Parent `32b18c27` CI run 42488635271 passed all applicable jobs, with original
logs retained separately in the same archive. Neither that older green run nor
the supplementary 71 passes relabel the current complete host-library failure.
The new-head CI run 42547547754 is a distinct pending validation.
