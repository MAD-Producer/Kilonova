# Session CPU resource observation: completed increment

Issue: #60, which remains open. Implementation tested:
`7e177912eab3a6e5c6f8f77d65485ad780b22390`, based on main `dc4fd44a`.
The publication successor changes documentation only. This increment is separate
from the earlier headless GPU latency study and does not relabel its measurements.

The explicit `resource_memory` JSON query, native Session method and public host
entry point now separate GPU logical texture capacity, known CPU Vec payload /
backing capacity and unknown external memory. The [API contract](../host-api/session-resource-memory.md)
describes the observation scopes and exclusions. Ordinary preview snapshots and
the existing texture-only query are unchanged.

## Actual acceptance

Ran the complete, unfiltered `cargo test --locked --offline -p motion-host -j1 -- --test-threads=1`
in an independent target on E:. Actual result: **132 passed, 0 failed, 0 ignored**,
including the seven new regressions below and the original GPU, host integration
and documentation tests. Provenance and the complete branch diff check also
passed. No assertion was removed, relaxed or skipped.

The [acceptance manifest](session-resource-memory-acceptance.json) records exact
commands, source, target, original log hashes, terminal exit codes and every test
summary. Original starts/logs/exit receipts remain under
`artifacts/pr-queue/issue60-session-memory-v1-*`. Compilation used one job,
disabled incremental/debuginfo through the recorded test environment, and kept
TEMP/TMP/build products on E:. This is correctness evidence, not a performance
comparison of build profiles.

| New test | Evidence established |
| --- | --- |
| Shared and distinct cached frames | Actual Arc alias references count one frame allocation; equal source/PTS/pixels in distinct allocations remain separate; spare Vec capacity counts; querying retains no Arc after return; eviction/clear releases observations |
| YUV cache frame | Both actual plane capacities count without making an RGBA copy |
| Controlled busy cache | Query returns while a mutex remains held; bytes are unknown, not zero ownership; the next query observes actual capacity after release |
| Poisoned cache | Actual poisoned mutex returns an error preserving object, source and phase |
| Preview proxies | Actual owned RGBA/YUV proxy capacity counts; expired weak source identity adds no decoded pixels; clearing releases the subtotal |
| Plugin JSON query | CPU payload/capacity stay separate from GPU/external values; clearing Vec length retains backing capacity; replacing vectors releases it; no graphics/project/revision/history change |
| Public host query | The public Session registry entry point returns the same read protocol without requiring a surface |

No new file logging is introduced under cache locks. Errors flow to the caller;
the existing desktop worker response path remains responsible for its default
Error file/stderr record. This increment verifies the returned poison cause; it
does not claim a separate new end-to-end logger acceptance run.

## Coverage and remaining issue scope

R60.1/R60.2 gain CPU video-cache/proxy and Session scratch/bundle observations,
with shared-frame deduplication and actual Vec capacity. R60.3 gains an explicit
diagnostic snapshot, not per-frame CPU resource sampling or window deadlines.
These are partial original-issue dispositions, not completion of the broad rows.

The response is sequential and partial, not an atomic frame peak or total
process RAM/physical VRAM. Retiring/in-flight decoder allocations, external
frozen readers, original-source PreviewInputReader caches, image worker/completion
pixels, font/mixer/resampling storage, expanded effect packages, inference and
allocator/container overhead still need observation. GPU buffers retain their
separate native admission query; do not add overlapping admission and texture
figures together. Driver and codec allocations remain null.

True file decode / window playback / physical pressure studies and Android
Adreno/Mali runs remain unaccepted. The previous six-case GPU study fixed its
own target/quality/source; it does not establish Float16/Float32 comparative
performance or the behavior of this later source. No optimization percentage or
FPS claim follows from the CPU tests.

No dependency, third-party asset, panel, token, style, layout or interaction file
is changed. DESIGN remediation rows are retained because this increment only
adds explicit diagnostic metadata and its tests.
