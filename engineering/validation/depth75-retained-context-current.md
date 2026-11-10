# Finite retained-context GPU comparison

Issue: #75. This explicit `motion-depth-wgsl --research-batch` route keeps model
resources, pipelines, bindings and fixed buffer capacities for one finite
sequence. It does not connect a persistent backend to the application, change
the existing host's cold request contract, qualify a cache result or enable
automatic quality selection. No panel, Android, dependency or asset change is
included.

## Executed scope and identities

The [protocol](depth75-retained-context-protocol.md) was fixed before execution.
Source was based on `ad799f2f634b5a3897c7df2e06053216db9e86a4`; all six changed
source/document files remained unchanged through original session 43396. The
validation retained its own exact bytes and SHA-256 before its first command.
The tested executable SHA-256 was
`0e3934a0284bd3e4f134e88e1f3a14d2f58e658266485b93285a8851f7b69c0a`.

The actual device was NVIDIA GeForce RTX 4070 Laptop GPU, Vulkan, driver 591.86.
The pinned model/program identities, actual worker/backend/runtime/plan, prepared
RGB identities and output dimensions are retained in the original receipts.
The six worker admission/panic targets passed 38 tests, zero failures, original
ignores or filtered tests. The worker built with the existing desktop FFmpeg and
AI-depth features, offline and locked. No existing assertion or deadline changed.

## Actual output equivalence

Captured reference RGB inputs 00 and 01 were independently hash-checked and
prepared using the unchanged authored tensor transform. Their prepared hashes
are distinct. The new binary produced cold A and cold B in separate processes.
Both outputs were remapped through the unchanged authored transform and matched
the complete pre-refactor 640x480 Float32 output digests retained by
`depth75-sustained-reference-v1/cohort-00`. Source clocks, dimensions, original
model and actual adapter identities were checked before that comparison.

One new process then executed `[A, B, A]` using one context preparation, 282
verified resource-upload starts and 13 distinct pipeline entries compiled once
in the same order as the cold controls. All three complete 518x518 Float32 GPU
readbacks matched their corresponding cold outputs exactly, including A after B.
Every output had 1,073,296 bytes, finite values, 429 encoded operations and one
final readback. The context retained 227,968,156 explicitly managed buffer bytes.
The readback was unmapped before its next use, and nested allocation/validation
scopes were collected before reuse. There was no CPU model-output injection.

## Limited timing observations

| Invocation | Whole command seconds | Context setup seconds | Per-input execution seconds | Submit through final readback seconds |
| --- | ---: | ---: | --- | --- |
| Cold A | 4.265 | 2.750 | not separately recorded | 0.432 |
| Cold B | 3.515 | 2.169 | not separately recorded | 0.414 |
| Retained A/B/A | 4.640 | 2.359 | 0.471 / 0.219 / 0.245 | 0.442 / 0.194 / 0.209 |

These are the actual recorded runs, not steady-state percentiles or realtime
qualification. Whole command includes process/adapter/setup/file overhead;
per-input execution includes input transfer, encoding, synchronization and output
handling. Submit-to-readback is not pure shader time. The first retained input
and all setup costs remain included. New standalone timings do not replace the
earlier complete host/transform/worker/remap sustained measurements.

The final queried process peaks were 175,345,664 resident bytes and 659,009,536
private-commit bytes. Per-input queries are cumulative self-process observations,
not per-frame-exclusive usage, final lifetime peaks, full GPU allocation or a
memory budget. A future persistent child overlapping preprocessing/remapping
requires different process accounting.

## Real failure and output controls

Twelve real invalid requests failed before adapter creation: malformed JSON,
oversized manifest, empty/oversized batch, wrong version/program/backend,
shape/dtype mismatch, unknown field, wrong digest and path escape. Their receipts
contain no adapter/context preparation, no adapter-request/allocation start,
no encoded operations and no output. Zero encoded operations alone was not used
as proof of early rejection.

An additional isolated `[A, B]` control mutated only its owned copy of B after
the first input's actual execution phase was observed. The worker retained one
completed A, refused input 1 with the original tensor digest error, exited 1 and
wrote no B output. The first A still matched cold A exactly. No immutable input,
threshold, model, backend or successful output was replaced.

All 13 declared failure commands returned their expected exit 1. Their original
causes, request/operation/phase/input index and elapsed times appeared in file
Error logs and stderr at the default log level. Native stdout remained empty;
Info starts preceded input reads/setup/GPU waits. There were no retries or
post-result changes to the comparison ceilings. Original session 43396 completed
with controller exit 0 in 139.156 seconds; source remained unchanged.

## Evidence and remaining work

The [manifest](data/depth75-retained-context-v1.json) and
[archive](data/depth75-retained-context-v1.zip) retain 111 members, 125,379 bytes,
SHA-256 `f5de6b7e241b03d3aa64414c017789112cf6fdbe5f15edd704999347e1cbf123`.
Every member passed CRC, length and SHA-256 readback verification. Contents include
the original controller, commands, manifests, phase/output/error receipts,
raw logs and the exact six frozen source/document files. No model, tensor, RGB,
depth array, executable or runtime bytes are distributed.

The two-image/three-input comparison proves the declared finite controls only.
The subsequent [application backend milestone](depth75-live-service-current.md)
records real Jobs reuse, cancellation and native failure controls separately;
it does not expand this finite run's original scope.
A live persistent application backend, cancellation/generation-aware lifetime,
concurrent process/device memory admission, application quality qualification,
accepted continuous-video cross-quality transitions and automatic tier selection
remain open. The baseline full maintained-workspace command still failed with
two test failures and two additional native target exits. This narrower run is
not a retry or replacement of that broad failure. PR #651 remains a draft.

## Existing resolve/attest CLI controls

After consuming the finite comparison, original session 86672 exercised the
same binary's existing `--resolve` and `--attest` routes with source frozen.
Resolution verified all 282 resources and returned the same actual execution
identity as the finite batch without encoding inference operations. Attested
A and B each completed 429 operations and one final readback; both entire output
digests matched their corresponding cold outputs. A changed-runtime identity
was rejected with its original cause before context preparation/inference, with
Error in file/stderr, empty stdout and no output. The four commands returned
0/0/0/1 as declared and completed in 19.437 seconds.

Original session 86672 was consumed with exit 0; source and binary were unchanged.
The [contract manifest](data/depth75-retained-contract-v1.json) and
[contract archive](data/depth75-retained-contract-v1.zip) retain 23
members, 47,956 bytes, SHA-256 `6459aa2c04425b604ed0ba39eae83fd4694f05319964db1ddf971ebb7b595be8`.
Every member passed CRC, length and SHA-256 readback. This verifies the existing
native CLI contracts, not an actual Rust Jobs/Session invocation, a persistent
application backend or a replacement for the failed complete-workspace run.
