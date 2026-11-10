# Retained depth context: finite research protocol

This is a proposed, explicitly selected research route for issue #75. It is not
an automatically selected application backend, a persistent service, a qualified
video-quality tier or a new cache identity. The existing host invokes the cold
single-input route. Android and panel changes are outside this work.

## Frozen comparison

The initial implementation is based on `ad799f2f634b5a3897c7df2e06053216db9e86a4`.
Before any native execution, record the exact changed source hashes, binary
digest, pinned static program and resource identities, actual adapter/backend,
input manifests and the original full-regression terminal receipt. Do not start
this build or inference while that original regression is live.

Use the already retained independently captured RGB inputs 00 and 01 from
`depth75-reference-inputs-v1`, verifying both against its original `inputs.json`.
Prepare each through the existing RGB/letterbox/normalization transform. Keep
the prepared Float32 input bytes, shape, transform receipt and SHA-256 unchanged
between cold and retained runs. Check that the two prepared input digests differ.
No model, photograph, weight or runtime acquisition is necessary.

Run the new binary's single-input route separately for A and B. Then run one
`--research-batch` invocation with `[A, B, A]`. Keep all three records, including
the first retained-context execution. Do not remove setup, initial execution,
slow samples or failures. Use the existing parent's 60-second native-child
allowance for each comparison command; a failure or timeout is a recorded result,
not permission to retry, raise a deadline or change a fixture.

Also remap the two new cold outputs using the unchanged authored transform and
compare their complete 640x480 Float32 output digests against the retained
pre-refactor `depth75-sustained-reference-v1/cohort-00` measurements and arrays.
Verify the original model, source clock, dimensions, adapter and array identities.
Same-binary cold/batch agreement alone cannot prove that the refactor preserved
the earlier implementation's output.

## Required checks

- The batch uses one process, one context preparation, the pinned 282 resource
  uploads and the same independently validated fixed allocation capacities.
  Required pipelines and bindings are constructed once. No extra explicitly
  managed input/output texture or buffer allocation may appear per frame; this
  does not measure wgpu/driver-internal command allocations.
- Every output is a real GPU readback. Its shape, byte length, finite values and
  digest are checked. A/B/A outputs must each match the corresponding same-binary
  cold output exactly. A failed comparison remains failed; do not introduce a
  tolerance after seeing the result. A/B/A covers changed-input pool reuse, not
  broad domain or temporal quality.
- Every frame waits for the final map, copies its owned output and unmaps the
  readback before the next frame. Nested validation/allocation errors must be
  consumed before proceeding. Failure preserves the primary cause and stops the
  batch. There are no retries or output substitutions.
- Record cold whole-command duration, context preparation, every retained-input
  execution and actual GPU submit-through-map duration separately. File admission,
  setup, transport and output serialization must not be called shader time.
  Three frames do not establish steady-state percentiles or realtime capability.
- A self-process high-water mark queried after each frame is a cumulative process
  observation. It is not exclusive per-frame memory, final lifetime peak, complete
  device allocation or the application's memory budget. The existing host plus
  largest sequential child memory calculation does not certify a future persistent
  child that overlaps preprocessing/remapping.

## Rejection and observation controls

Keep the default log level. Successful commands must have empty stdout. Logs
must contain an Info start before setup/input reads/GPU waits. Malformed manifest,
oversized manifest, empty/oversized batch, wrong model identity, wrong backend, shape/dtype errors,
and unknown fields must fail before GPU inference. Real digest mismatch and path
escape controls must retain Error in both the file and stderr, with no completed
input. Verify absent adapter/context records and absent adapter-request/allocation
starts as well as zero encoded operations; zero encoded operations alone cannot
prove that a request was rejected before adapter creation. Error lines identify
the request, operation, phase, input index and elapsed time. A failure after one completed input must preserve that first result and
identify the rejected input; it must not claim the entire batch succeeded.

Retain the exact validation controller bytes and their SHA-256 before its first
command, alongside the changed source bindings. A mutable external script path
alone is not a reproducible record of the executed comparison.

The pure admission tests check bounds, order, duplicate-input preservation and
strict schema/execution admission. They cannot prove GPU reuse or output quality.
Compilation, formatting and patch applicability are preparation checks only.

## Current disposition

Original session 43396 completed and was consumed with exit 0 after the
baseline's original full-regression session 87729 was consumed with exit 1.
The changed source remained frozen through compilation, 38 admission/panic tests,
two cold inputs, the real A/B/A comparison and 13 actual failure controls.
See the [actual results](depth75-retained-context-current.md) for exact output
equivalence, measured setup/input times, evidence and limits. The original full
regression failures remain failures. This finite route is not connected as an
application persistent backend, a qualified video-quality switch or automatic
tier selection; issue completion is not claimed.
