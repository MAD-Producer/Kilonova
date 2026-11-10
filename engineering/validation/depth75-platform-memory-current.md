# Current-source desktop process-memory CI evidence

Issue: #75. Source: `ad799f2f634b5a3897c7df2e06053216db9e86a4`.
The fetched main baseline `ac65d279c57626d7aa7e28061a1a7fdf7889c68a` is an
ancestor of this source. This record does not certify model or application
capability and does not waive local failures.

## Terminal remote checks

[Run 42556322285](https://github.com/MAD-Producer/Kilonova/actions/runs/42556322285)
completed successfully. All eight applicable jobs passed: Rust and GPU,
dependency audit, and desktop tests and builds on Ubuntu, Windows and macOS
Intel. The libav and publish jobs were skipped by the workflow; they are not
reported as additional successful checks. Android is outside the maintained
acceptance scope at the user's direction.

The original raw test logs were checked for exactly one successful occurrence
of each of these five controls on each of the three desktop platforms:

- `process_memory::tests::sequential_stages_keep_unknowns_and_do_not_sum_nonconcurrent_child_peaks`
- `process_memory::tests::actual_owned_child_retains_touched_allocation_high_water_marks`
- `process_memory::tests::peak_formats_keep_missing_platform_counters_unknown`
- `process_memory::tests::unavailable_retains_bounded_cause_instead_of_zero_usage`
- `depth::onnx::profile_tests::memory_report_retains_unavailable_and_rejects_inconsistent_counters`

These are 15 named successful records, not 15 independent application-memory
qualifications. The current shared module is `process_memory`; the older
`depth::capability::memory` module re-exports that implementation. Earlier-source
test names and green checks were not substituted for current-source records.

## Evidence binding

The [manifest](data/depth75-current-platform-memory-v1.json) describes the
[archive](data/depth75-current-platform-memory-v1.zip): 19 members, 910,319 bytes,
SHA-256 `a5554a3f78378ea310615122a4f22b0fd546ad9ff253cf838d6c1c5afca82bcd`.
It retains run/job metadata, eight original job logs, the collection controller,
selected source snapshots and the frozen complete tracked-source hash mapping.
Every archived member was independently read back with CRC, length and SHA-256
verification. Selected snapshots and source hashes are not a complete source
archive or executable distribution. No models, weights or dataset arrays are
included.

The original final collector handle 61996 became unavailable after its output
was truncated. Its exit code is not inferred. The completed collection record,
terminal run metadata, final manifest and all archived bytes were independently
verified without restarting the collector or any CI/test workload.

## Local failures remain open

The original complete maintained-workspace command, session 87729, ended with
controller exit 1 and Cargo exit 101. Source, assertions, original ignores,
features and deadlines were unchanged. All 449 expected invocations were
observed, with 447 normal summaries: 5,274 passed, two failed, 78 original ignored
and zero filtered tests. Its desktop main target reported 1,778 passed, two
failed and 72 original ignored tests. The accumulator-admission parent reported an
unsuccessful child exit without printing the child's status code. The sampling
playback parent retained child exit `0xc0000005`; it did not retain an observed
frame-increase assertion failure.

The original Cargo stderr additionally records native exits
`0xc0000005, STATUS_ACCESS_VIOLATION` for the `packages` and `pointwise_fusion`
integration targets. `--no-fail-fast` allowed the remaining original targets to
run. Both crashed targets lack normal test summaries. The original terminal
handle was consumed without retry. The
[complete-command record](depth75-full-regression-current.md) retains raw output,
source bindings and final target accounting. This is a failed local acceptance.

Missing Epic overlay JSON messages appeared in some child output. Their
causation has not been established. No foreign module inspection, debugger
bypass, environment workaround, failure retry, assertion relaxation or foreign
process termination was performed. Successful remote checks do not repair or
erase these local failures.

## Remaining limits

OS self-process high-water observations remain incomplete lifetime samples.
Sequential host-plus-largest-child accounting is not simultaneous physical
usage, complete GPU memory, allocator ownership or a certified budget. A future
persistent inference child overlapping preprocessing/remapping needs different
accounting. Neither these CI checks nor the finite retained-context experiment
qualify that architecture. Thermal flags, actual cross-resolution alignment
refusals, live video quality transitions and automatic tier selection retain
their separate acceptance requirements. PR #651 remains a draft.
