# Retained WGSL application backend (#75)

This milestone connects the previously measured fixed GPU context to the real
bounded `Jobs` backend interface. It is explicitly selected with
`Wgsl::new_retained(Config)`; `Wgsl::new` retains its single-inference process
behavior. It does not enable automatic quality selection or qualify a device.

## Implementation

The owned native process starts with `--serve <execution> <resolve-request>
<new-output-directory>`, verifies the actual execution against the expected
native identity, and prepares weights, pipelines, bindings and buffers once.
Each input uses the same ordered compute graph and unmapped final readback.

The transport has one command and one acknowledgment in an owned workspace.
Temporary files and rename publish complete messages; there is no unbounded IPC
queue. Commands are limited to 16 KiB and acknowledgments to 4 KiB. The owner
checks cancellation and direct-child exit every 10 ms. Schema, monotonic
sequence, child PID, execution, program/backend, fresh owned request/output
paths, tensor dimensions, finite values and digests are checked. Worker stdout
is empty; bounded pipe readers retain diagnostics. Info starts and in-flight
state precede input admission, model/device work and GPU waits. Unparseable
commands have an explicit unknown request; parsed invalid commands retain their
own sequence and admission phase, rather than the previous resource upload ID.

Cancellation or a transport/GPU failure terminates and reaps only this backend's
direct child. This backend then refuses further work. There is no automatic
restart, retry, fallback, engine timeout or render-session device transfer.
Creating a new backend is an explicit caller action. Thus this experimental
mode is not yet a continuous-video cancellation/recovery policy.

The combined execution fingerprint includes the retained service mode, so cold
and retained process profiles/cache identities cannot masquerade as each other.
Individual host/child lifetime OS peaks remain available. The new
`WgslRetained` memory observation marks overlapping resident-worker/transform
stages; the sequential-child resident sum is unavailable for this mode. This
does not establish a concurrent whole-host/device memory budget.

## Actual Jobs and failure controls

The final frozen source was base `a459112a` plus the 12 source files whose
SHA-256 digests are recorded in the archive. Original session 87782 completed
with exit 0 and unchanged source in 40.406 seconds, including build/checks:

- 22 host WGSL unit tests and five process-memory tests passed
- All seven worker admission/panic targets passed: 40 tests
- One explicit real-hardware Jobs test passed; it is opt-in because CI must not
  implicitly acquire a Python runtime, model or captured RGB fixture

The 68 passed tests have zero failed or ignored results within the explicitly
selected invocations; unit-test filtering is retained in the raw summaries.
This is not a full workspace test count. The real control used two distinct
existing 640x480 captured RGB inputs as A/B/A. All three complete Float32 outputs
matched the earlier cold-path full-output digests exactly. One native PID
(48908) and one context preparation served the three inputs. The actual
retained buffer capacity was 227,968,156 bytes for each measured result; the
inapplicable sequential process sum remained unknown.

The test observed the fourth input's actual submitted GPU/readback phase before
canceling. The result was discarded, the direct child and owned workspace were
cleaned up, and a subsequent request was explicitly refused without restarting.
Counts remained three completed, one canceled and one expected refused request.
This does not prove an accepted newer frame after a cancellation or a full
Session/export path.

Original session 12787 then exercised nine actual native failure commands on
the same binary and source: changed expected execution, malformed/oversized
mailbox data, unknown field, wrong sequence, escaping path, changed program,
wrong tensor digest and replay after one completed input. All returned the
declared exit 1. The replay retained its first complete correct output; none of
the other failed input commands produced a depth output. Changed execution was
refused before fixed-pool allocation. Original causes and correct available
request/operation/phase context appeared in default-level file Error logs and
stderr; native stdout was empty. No assertion, threshold or retry was used to
turn an unexpected result into acceptance.

## Original failure and correction

Original session 29112 failed with controller exit 1/Cargo exit 101. It exposed
an actual ownership error: dropping the service workspace before closing the
nested request directory turned normal cancellation into a secondary missing
directory cleanup error. That failed run and its unchanged assertions/deadlines
remain archived. The fix reaps the child while keeping its workspace alive,
closes the nested directory, and then drops the workspace. Session 88534 passed
after this source correction. Session 87782 additionally verified the corrected
admission observation context. The earlier nine native controls (session 14447)
and their final-source repetitions (12787) are separately identified.

All 12 original failed-source files were recovered from the recorded correction
and accepted only when their bytes matched the original failed controller's
frozen SHA-256 hashes. The first reconstruction observer rejected a multiline
formatting mismatch before publishing mismatched source; the corrected observer
verified every original hash. No test or model was rerun for that reconstruction.

## Evidence and remaining boundaries

The [manifest](data/depth75-live-service-v1.json) and
[archive](data/depth75-live-service-v1.zip) retain 244 authored source, command,
receipt and raw-log members: 505,947 bytes, SHA-256
`2b91bfaa1beada8c4d7885b2dd5a7443ab7260d352a1087c2e20921036de4795`.
Every member passed CRC, length and SHA-256 readback. No model, tensor, RGB,
depth array, executable or installed runtime is distributed.

Still pending are actual retained-mode persistent-cache reopening/isolation,
qualified Session installation/export, concurrent memory admission, sustained
latency/thermal measurements, accepted cross-quality video alignment and
automatic selection/recovery policy. The current source has not completed full
maintained-workspace regression or cross-platform CI. The original broad
regression failures, prior rejected cross-resolution fits and thermal outcomes
remain failures/refusals. No Android, panels, UI or existing assertion changes
are included. #75 and draft PR #651 are not declared complete by these controls.
