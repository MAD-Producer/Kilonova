# Owned depth child failure diagnostics (#75)

This change improves failure evidence for the actual depth transform subprocess.
It does not fix or waive the previously observed native access violations. The
source base is `84de8b11`; the archive binds all five changed files by SHA-256.
No model, dependency, runtime or dataset acquisition was performed.

## Observation changes

All three owned Python transform invocations (plain, resolve and attest) enable
Python's fault handler through `-X faulthandler`, retaining their existing
isolated mode and bytecode suppression. No exception is swallowed and no retry,
restart, fallback or process workaround is added.

The child stderr reader retains at most 8,192 bytes, divided between the initial
stack header and the final cause, with a middle-omission marker. Failed child
exits log that bounded diagnostic with PID and exit status at Error; the existing
desktop sink writes Error to file and stderr. The returned diagnostic stays
within 512 Unicode characters. Stdout remains the separate bounded protocol
channel. Short diagnostics remain exact, and the existing error-length assertion
is preserved.

An idle retained GPU service's `status=running` receipt no longer displaces the
original Python prepare failure. Actual failed native receipts and recorded
native panics still take precedence over a long secondary pipe diagnostic.
Cleanup failures continue to preserve the primary error. No UI or panel changes
are included.

## Original full-host failure remains failed

Original controller session 14287 ran the full host library target with the
`ai-depth` feature, offline and serially. Its command exited 3221225477 after
208.203 seconds; the raw Cargo diagnostic reports `0xc0000005`
(`STATUS_ACCESS_VIOLATION`). The controller exited 1. The test target produced
no normal final summary. Its last progress line names
`video_frames::frame_blend_tests::exposure_required_decoder_error_is_terminal_and_new_explicit_reader_can_recover`;
that line alone does not establish the fault location or cause.

The four new diagnostic unit controls had printed `ok` before the abnormal
exit. The Python probe, worker build and cache control had not executed. This
failed host run was not rerun to obtain a green result. Its complete raw output,
original source bindings and consumed-session receipt are preserved. The prior
Python prepare access violation in session 79852 also remains unresolved. There
is no established common cause between these two faults.

## Separately scoped controls

After consuming the failed original controller, session 24071 validated the
changed depth components and the previously unexecuted controls. Adding PID to
the failed-child log was the only subsequent product-source change; both source
snapshots are retained. Every command below exited 0 on an unchanged snapshot:

- Depth component selection: 59 passed, 0 failed, 1 opt-in test ignored and 226
  tests filtered out; 1.40 seconds in the test target
- Explicit installed-Python probe: 1 passed, 0 failed/ignored and 285 filtered
  out; the three actual child invocations verified fault-handler enablement,
  an explicitly dumped owned-source stack, the original nonzero exception cause
  and empty protocol stdout
- Actual `motion-depth-wgsl` build with FFmpeg and AI-depth features; worker
  SHA-256 `033b161c1789a09011162630a24430e79271b27bcddc3bb64f39986bbdef1158`
- Actual retained/cold persistent-cache integration: 1 passed, 0 failed/ignored
  and 1 filtered out; 44.22 seconds in the test target, two real inference calls,
  two mode-specific cache entries, no extra inference on reopening or corrupt
  entry refusal, and all owned service directories removed

The cache control retained the original SHA-256 corruption error in default-level
file output and stderr. Its complete depth output remained
`edb04b097a7e16b1589ce19ae7455700a357978ac01381ffbd9df703300d04af`.
The controller completed in 124.110 seconds and its original tool result was
consumed with exit 0. It did not rerun the failed Jobs A/B/A cancellation control.

The Python probe uses an authored script that dumps its own stack and raises an
ordinary exception. It proves observer enablement and diagnostic transport; it
does not reproduce a native access violation, prove its stack will be captured
under every failure, or establish that either original native fault is fixed.
The narrower passes do not replace the failed full-host or workspace acceptance.
No existing assertions, numerical thresholds, timeouts or ignores were weakened.

## Evidence and remaining boundaries

The [manifest](data/depth75-child-diagnostics-v1.json) and
[archive](data/depth75-child-diagnostics-v1.zip) contain 35 authored source,
controller, receipt and raw-log members: 84,131 bytes, SHA-256
`e6cdb7391b534f2f22f5c3ed7996817239c42c26d9ae881bf5906ab6a77c265d`.
Every member passed CRC, length and SHA-256 readback. No RGB, depth/cache payload,
model, native binary or installed runtime is distributed.

Native fault diagnosis, qualified Session installation/export, concurrency-aware
memory admission, sustained retained-mode latency/thermal qualification,
cross-quality video alignment/recovery, and current-source full-workspace and
cross-platform acceptance remain open. Android is retired and outside the
maintained delivery scope. #75 and draft PR #651 remain incomplete.
