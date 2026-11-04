# Expression interruption failure phases

Status: observation increment verified on the parent clock branch. The original
PR 597 CI timeout remains unresolved; this is not a deadline or scheduling fix.

At head `644c6396`, CI job 126486711281 failed in the unchanged
`deferred_nonfinite_sampling_reports_error_without_poisoning_later_evaluation_or_history`
test while admitting a valid frame-zero expression. The generic interruption
error did not distinguish cancellation, the frame deadline, or the execution
deadline. The complete original job log is retained in the archive below.

The same error now carries the first observed phase (`context_setup`,
`bindings_and_module`, or `execution`), interruption condition, elapsed
microseconds, and frame time remaining at entry. The condition follows the
existing cancellation/frame/execution check order; elapsed wall time does not
prove CPU cost or the original CI root cause. No expression source or user text
is recorded. Formatting occurs only on failure. Existing deadlines remain
20 ms per root execution and 100 ms per frame; memory/stack ceilings,
cancellation, reference-failure precedence, return values and recovery policy
are unchanged. There is no file IO or unconditional logging in this helper.
The contextual error continues through the existing host/worker error sinks.

A real infinite expression verifies interruption details, unchanged project
state and successful later evaluation. A deterministic unit test verifies
first-phase/cause retention and exact elapsed context. All original assertions
remain unchanged. Complete core unit tests and the four selected expression
integration targets executed: **64 passed, zero failed, one original manual
performance test ignored, zero filtered**.

The Cargo process exited zero. The controller subsequently exited one because
its receipt assertion incorrectly required zero original ignores in the core
unit target. An independent audit parsed all five target summaries and checked
the frozen source hashes without rerunning any tests. Both original failure
receipt and additive audit remain in the
[archive](data/expression-budget-trace-v1.zip), with a
[hash receipt](data/expression-budget-trace-v1.json). This is not complete
desktop sink validation, full workspace regression, cross-platform acceptance
or completion of issues 85/86/88. The concurrently running full desktop/UI gate
uses its unchanged earlier child source and does not cover this increment.
