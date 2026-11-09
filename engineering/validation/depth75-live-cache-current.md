# Actual retained-mode persistent cache controls (#75)

This control extends the [retained Jobs milestone](depth75-live-service-current.md)
without changing product code or its validated native worker binary. Source was
base `ead5f81c` plus the recorded integration-test change. The worker SHA-256 was
`71843dec635a4ebad8f9537a5abd1a53ff1b18300d4562994266dc4b931f4ac1`.
No model, runtime or data acquisition was performed.

## Original Jobs failure remains failed

Original session 79852 first attempted the existing real Jobs A/B/A cancellation
control before the new cache case. It failed with controller exit 1/Cargo exit
101: the second input's owned Python `prepare` child exited with status
3221225477 (`0xc0000005`, access violation). The native retained service was idle
waiting for another request. Its coincident missing-EOS-JSON diagnostics do not
establish a cause for the Python child fault. The original request, phase,
elapsed time, exit status, raw outputs and unchanged source binding are archived.

The cache case had not executed in that controller. The failed Jobs test was not
retried, relabeled as passed, or given different assertions/deadlines. Earlier
Jobs controls passed on the previously recorded test source; they do not erase
this newly observed failure. No third-party overlay inspection, debugger,
registry change, environment workaround or foreign process termination followed.
The native access violation remains unresolved.

## First execution of the independent cache case

After consuming 79852 with exit 1, original session 89099 executed only the
previously unexecuted cache case, on the same source and native binary. It passed
with controller exit 0/Cargo exit 0 in 46.281 seconds (45.59 seconds in the test).
Its one selected test passed, with zero failed/ignored and one explicitly
filtered test: the failed Jobs control was not rerun. This narrower pass is not
acceptance of the entire source or a replacement for the original failed run.

The test used real `Jobs::start_cached` and real WGSL/transform processes with an
existing captured 640x480 RGB image. A bounded observing wrapper delegated all
execution identity and inference calls to the actual backend; it supplied no
outputs or execution identities itself. The complete output matched SHA-256
`edb04b097a7e16b1589ce19ae7455700a357978ac01381ffbd9df703300d04af`.

- First retained request missed and ran one real inference
- The same retained Jobs instance hit on a repeated request; its service still
  reported exactly one completed inference
- A newly opened retained Jobs instance hit the persisted result with zero new
  inference calls and zero completed service inputs; the restored resource had
  fresh ownership identity and the same complete output
- Cold-mode Jobs missed the retained entry and ran a separate real inference;
  provider and plan stayed equal while the runtime fingerprints differed
- The cache contained two distinct execution-mode entries
- Corrupting only this control's retained entry produced the original content
  SHA-256 error in default-level file Error output and stderr, with zero extra
  inference calls and zero completed inputs in that new retained service

Total inference calls were two: one retained and one cold. All owned retained
service workspaces were removed after their direct children were reaped. A
cache hit still starts/attests a service; this test does not claim allocation-free
or instant reopening. Reading observational receipts tolerates an incomplete
concurrent file write under the original 60-second observation deadline; no
assertion or numerical threshold was weakened.

## Evidence and limits

The [manifest](data/depth75-live-cache-v1.json) and
[archive](data/depth75-live-cache-v1.zip) retain 19 authored test/controller,
receipt and raw-log members: 33,450 bytes, SHA-256
`15e9a2fd07468c331d0b5e7610b5cc13a00381ff44c673015389587ed6c29c22`.
Every member passed CRC, length and SHA-256 readback. Failed session 79852 and
independent passed session 89099 are both included. No RGB, depth, cache payload,
model, native executable or installed runtime is distributed.

Remaining work includes the observed native child fault, qualified Session
installation/export, concurrency-aware memory admission, sustained latency and
thermal qualification, accepted cross-quality video alignment, automatic tier
selection/recovery, and full maintained-workspace/cross-platform acceptance.
Prior broad regression failures and rejected numerical/thermal qualifications
remain unchanged. No Android, panel, UI or product-source change is included in
this test-only milestone. #75 and draft PR #651 remain incomplete.
