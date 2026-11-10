# Failed actual retained fixed32 cohort (#75)

The existing `depth_reference` example now accepts the explicit
`wgsl-retained` backend. Its fixed 32 input pairs, original order, preparation
identity, per-request 60-second wait, zero-reuse assertions and output protocol
are unchanged. `wgsl` still selects the cold backend. This is a measurement
entry point, not an automatic fallback or device qualification change.

## Original execution

Source was `53c3280d` plus the archived example change. The three complete
example unit tests passed, with no failure, ignore or filter, and the example
built successfully offline. Original controller session 84735 then ran the
retained cohort once. The driver exited 1 after 93.046 seconds; the controller
also exited 1. Source and native worker bytes were unchanged, and the original
tool result was consumed before further edits.

The first 27 requests completed. Request 28 (`asset=750027`, revision 1, source
PTS `1305031454259627000/1000000000`) failed during `remap`: its owned Python
child PID 24168 exited 3221225477 (`0xc0000005`). Four later inputs did not run.
The report remains `status=failed` with a null final summary. No request was
retried, skipped, substituted or relabeled as successful.

## Actual fatal-stack evidence

The previously added fault handler and bounded stderr retention captured this
real access violation, rather than only an ordinary exception probe. Both the
default-level file log and console stderr contain the same original child PID,
exit status and `Windows fatal exception: access violation` header. The trace
places the current Python frame in `typing.py:1721`, called through its hashing
and subscription helpers while importing NumPy's `_typing/_char_codes.py:204`.
The owned transform entry appears at `depth_tensor.py:100`, importing NumPy for
the remap phase. This establishes the observed call path, not a native root cause
or proof that NumPy, Python typing, the GPU driver or an overlay caused the fault.

The returned error preserves `WGSL phase=remap`, elapsed time and the fatal
header before the idle retained service's secondary diagnostic. Protocol stdout
is empty. No debugger, module/registry inspection, environment workaround,
foreign-process termination or exception suppression was used. Earlier prepare
and full-host access violations remain separate unresolved observations.

## Sealed completed-prefix audit

After consuming the failed run, an independent read-only reducer checked all
27 completed output files against their recorded hashes and all complete
Float32 bytes against the sealed historical cold cohort. Every completed
output matched exactly; the comparison performs no new inference. All 27
observations identify one retained native worker PID, fixed managed GPU buffer
capacity 227,968,156 bytes, and an explicitly unknown overlapping-process
resident sum. The owned service workspace was removed on failure.

This prefix result does not complete the planned 32 requests, establish a
full-cohort percentile, replace the failed workload or qualify quality, thermal
stability, memory admission or real-time operation. Historical cold timings
would not be a controlled speedup comparison even if the new cohort completed.

## Evidence and next boundary

The [manifest](data/depth75-retained-cohort-v1.json) and
[archive](data/depth75-retained-cohort-v1.zip) retain 15 authored source,
controller, reducer, receipt and raw-log members: 33,424 bytes, SHA-256
`d790a43898aee42af623580856b3b56a125733d7cfdb328820b91571941f24a9`.
Every member passed CRC, length and SHA-256 readback. No RGB, depth/cache payload,
model, generated image, native executable or installed runtime is distributed.

The next implementation boundary is the transform execution path and its
repeated numerical-runtime imports. Any native replacement must retain a
separate truthful execution identity and independently prove preprocessing and
remapping numerics; removing a failing path would not establish the original
native crash's root cause. No such replacement or crash fix is claimed here.
Qualified Session/export, concurrent whole-memory budgets, retained sustained
load, accepted video cross-quality transitions and full current-source
regression/CI remain pending. No Android, panels, UI, dependency or existing
test-assertion changes are included. #75 remains incomplete.
