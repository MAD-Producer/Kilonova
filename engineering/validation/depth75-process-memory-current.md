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
Earlier WGSL builds report no inference-child observation; this is unknown, not zero.

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

## Native WGSL extension (Windows verified; other desktop OS checks pending)

The shared Rust `process_memory` module exposes the same typed self-process
query independently of the optional AI-depth feature. Existing depth APIs
re-export its types. Windows uses the already locked windows-sys 0.61.2 APIs;
Linux/macOS use the already locked libc 0.2.190 `getrusage` binding. Their direct
host dependencies and unmodified MIT notices are recorded in ATTRIBUTION/NOTICE.
No new package version or download is required.

The completed native WGSL worker adds its own observation immediately before
the existing final receipt write. The backend validates and retains it through
Costs and measured Jobs. It does not query other processes or add a per-node
sampler. The real native-child control touches and releases 64 MiB and checks
that OS high-water marks remain monotonic. Cross-platform control execution
remains to be verified; the earlier ONNX measurements do not prove this
extension. Parent/resolution/transform processes,
physical device allocation and complete memory admission remain open.

At `b388ada3`, the complete host library, capability and Session targets passed
277, 39 and 32 tests: 348 passes, zero failures, ignores or filtering. This
includes the actual native child allocation/release control and the unchanged
report-validation controls. The host also checked without default features or
AI-depth. All three actual probes and the WGSL worker then built successfully.

Both actual routes completed two uncached requests on the same frozen 640x480
reference RGB. Existing models, preprocessing, deadlines and assertions were
preserved. Every distinct inference child reported OS counters through Costs,
measured Jobs and the actual rebuilt capability probe:

| Route | Inference PID | Peak resident bytes | Peak private commit bytes |
| --- | --- | ---: | ---: |
| CPU266 | 36808 | 355,844,096 | 1,113,554,944 |
| CPU266 | 15940 | 355,688,448 | 1,113,362,432 |
| WGSL518 | 34288 | 171,118,592 | 652,804,096 |
| WGSL518 | 38796 | 171,106,304 | 653,156,352 |

The WGSL observations remain separate from the validated 227,968,156-byte
managed GPU buffer capacity. They do not measure device allocation, parent,
identity-resolution or Python transform processes. Each query occurs before
final receipt serialization/teardown, not after process exit. Repeated input
probes do not establish video quality, numerical equivalence or thermal tiers.
Default Info starts were present in the real file logs; intended JSON was the
only CLI stdout. Original GPU loader diagnostics were retained, not filtered.

Original session 9625 was consumed with exit 0. Every tracked source hash
remained unchanged. The [manifest](data/depth75-native-process-memory-v1.json)
and [archive](data/depth75-native-process-memory-v1.zip) preserve commands,
native output, actual CPU/GPU observations, logs and source snapshots. All
members were read back and SHA-256 verified: 402,078 bytes, archive SHA-256
`c86761299961dd25c6961cc3069c3e5d9e384bd78ba00207f5aa88de3072a356`.
The earlier complete library failure remains a distinct failed run; this new
source's pass neither establishes its cause nor changes its result. This is
not a full maintained-workspace pass or cross-platform acceptance for b388ada3.

## Host and direct-child phase observations (validation pending)

The measured Jobs snapshot now queries the host process after backend/output
ownership checks and before downstream alignment. It retains a fixed-size
ONNX layout (resolution/inference) or WGSL layout (transform resolution/native
resolution/prepare/inference/remap). Every missing or unavailable phase remains
explicit; generic backends without phase observations return no snapshot.
The measurements never enter cache execution identities or quality admission.

The resolver/transform workers return their own OS counters in the existing
bounded reports. The native resolver adds counters to its existing receipt.
No new process, periodic sampler, Present logging, dependency or asset is added.
Old resolution memory is cleared before every resolution attempt; observations
are retained only alongside the corresponding successfully resolved execution.

The optional observed resident sum adds the host queried high-water mark to
the largest queried direct-child high-water mark: these backend phases execute
sequentially. It does not add peaks for nonconcurrent children. A missing,
unavailable, invalid, host-PID child counter or integer overflow returns unknown.
It is not an upper bound for final process lifetimes: queries precede final
serialization/exit. It also excludes descendants, other live Jobs instances and
device allocation. The host peak includes all threads and preexisting activity
in that host, without attributing it to this request. Full memory qualification
and the original issue's remaining requirements are still pending.

New controls cover each missing/failed child slot, nonconcurrent aggregation,
overflow, fixed report bounds, unchanged execution identity, actual resolver
self-counters, and snapshot/Profile retention. Original assertions and budgets
remain unchanged. Complete tools/native commands and actual CPU/GPU probes are
required before this extension is described as verified.
