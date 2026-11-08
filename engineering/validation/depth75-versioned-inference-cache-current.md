# Versioned depth inference cache: current increment

Baseline: main `7361d4d313db89580137f2549e0a94fd9165ff96` (merged #645).
This is an implementation and scoped acceptance record, not completion of #75
or #88. Android is not maintained and is excluded. No UI/panel, dependency or
third-party asset change is included. Separately installed licensed Small
weights remain outside product distribution.

The explicit `Jobs::start_cached` / `Session::configure_depth_cached_in` APIs
use a separate application-owned directory, with at most 2 GiB and 4,096 entries.
Each entry seals the current wire version, exact prepared RGB fingerprint,
canonical source PTS, geometry, model weight/revision/processing, actual
execution identity and raw scalar/validity/confidence planes. Limits and exact
encoded content digests are checked before plane allocation. Restores produce
fresh resource identities; no calibration, affine coefficients or admission
proof is persisted. Existing Session retention, video spill and frozen-export
budgets stay separate.

Only an absent entry is a miss that permits inference. Corruption, quota,
contention, stale lock or unsupported attestation fails explicitly. Atomic
publication does not overwrite an existing entry. There is no implicit retry,
eviction or stale-lock removal. Application-owned cache disposal is an external
lifecycle responsibility. Worker in-flight phases and default Info starts are
published/flushed before resolve/read/write; failure logging retains the
request, asset, revision, phase, elapsed time and original cause. Cache hit/miss
counts saturate over the Jobs lifetime and do not reset on seek.

Production attestation currently supports an explicitly exclusive CPU ONNX
session. Before lookup it verifies actual model bytes, shape and provider, and
hashes installed runtime/numerical package code and native dependencies,
pre/postprocessing procedure, CPU features and numerical environment. A miss
re-attests inside the executing child before publishing a result. Accelerator
provider registration is insufficient to prove node assignment: WGSL and
non-CPU ONNX persistent caching are explicitly unsupported at this stage, while
ordinary uncached backend operation remains available. Resolution loads a model
and runtime even for a hit; no latency or memory improvement is claimed without
measurement of that complete path.

## Actual scoped qualification

- Controller `depth75-cache-target-v1` was interrupted during compilation; tool
  handle 8680 was missing and its recorded controller/Cargo PIDs were absent.
  Partial outputs and executed source are retained without a fabricated exit or
  completed-test claim
- Actual session 14946 (`depth75-cache-target-v2`) was consumed with exit 0:
  all host unit, depth_jobs and depth_session targets passed, 302 tests total,
  zero failures/ignores/filters; all seven new cache controls passed. The
  feature-off check and existing nine Python export-admission tests passed.
  Source remained unchanged throughout qualification
- Actual session 6236 (`depth75-cache-file-v1`) was consumed with exit 0:
  separately installed Depth Anything V2 Small 266 ONNX model, digest
  `07eb7aeb46a755961e7dee25cd71c0067b12636d5b0fb3b96e20cc40fe5c12a1`;
  first inference 238.8342 ms; a new worker with a new ONNX backend restored all
  96 output scalar values bit-for-bit with fresh resource identity and zero
  inference cost. Total actual inference calls stayed at one, including after
  deliberate finite cache corruption was refused
- The actual production logging sink, default Info with MOTION_LOG unset,
  exposed a resolve start in its file before the controlled backend returned,
  without a client flush. The actual corruption error was emitted by the worker
  to both file and stderr; stdout was exactly one JSON line. No Engine request
  context, alternate stack size, altered backend or relaxed assertion is claimed

Original outputs, source archive, lockfile/fixture and receipts are preserved in
`artifacts/pr-queue/depth75-cache-qualified-increment-v1/evidence.zip`:
1,140,690 bytes, SHA-256
`0b1bd25da8e0be1d0a3e797847394e4ee72f70a7e917c457ac959a4d2a9da837`.
All members were read back and verified. This is a local evidence path; it is
not a portable repository download or cross-platform qualification.

## Required follow-up before final cache acceptance

- Exercise actual closed/reopened Session configuration, saved calibration,
  current-reference refit and exported pixel equality through the new APIs
- Ensure existing Session video-spill restore cannot bypass changed persistent
  execution identity; add a dedicated video control before claiming video reuse
- Bound sibling site-packages enumeration itself and test runtime identity
  rejection/change paths in addition to the already bounded package traversal
- Qualify WGSL/accelerator execution identity independently; do not guess it
  from a requested hardware name or infer CPU coverage applies to it
- Run complete changed-crate regression, desktop integration and exact-source
  cross-platform CI after final changes and main alignment

Measured quality/capability selection, actual continuous-video drift/jump and
sustained/thermal qualification remain #75 work. #88 photographed tracking
quality remains open. Synthetic RGB cache input is not inference-quality proof.
