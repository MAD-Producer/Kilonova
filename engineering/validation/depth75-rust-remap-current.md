# Explicit Rust host remap (#75)

`Wgsl::new_retained_rust_remap(Config)` explicitly selects the retained GPU
backend with an independently authored scalar Rust output remap. The example
accepts `wgsl-retained-rust-remap`. Existing cold and retained Python modes
remain selectable and unchanged; there is no automatic retry or fallback.
Python/OpenCV still prepare RGB inputs and attest their numerical identity.
The original [failed Python-mode run](depth75-retained-cohort-current.md),
including request 28's fatal access violation, remains failed and undiagnosed.
This implementation does not establish that fault's native root cause.

## Procedure and ownership

The host procedure validates a finite 518-square plane, crops the centered
letterbox rectangle and uses align-corners bilinear interpolation with explicit
Float32 intermediates. It checks cancellation before input access, per output
row and during bounded output writes, verifies the input digest, refuses an
existing output publication and preserves original errors. Dimensions are
bounded to 1..4096. It does not copy OpenCV implementation code or acquire any
dependency, asset or runtime.

The execution identity binds the owned host executable bytes, OS, architecture
and remap source procedure in addition to the original GPU/runtime identity.
Both runtime and procedure keys differ from the Python-only mode. A previously
resolved host identity mismatch closes the owned service instead of restarting
it. `WgslRetainedHostRemap` records the host remap under the host PID, not as a
fictional child. Overlapping-process resident sums remain explicitly unknown;
227,968,156 bytes is managed retained GPU buffer capacity, not total memory.

## Original controls and actual fixed32 result

Source was `870693eb` plus the five source files sealed in the archive. Original
controller session 73737 completed with exit 0 and was consumed before edits.
The WGSL component selection passed 30 tests, with its one existing opt-in
ignore and 262 filtered tests; memory components passed 6 with 287 filtered;
the complete example target passed 3, with no ignore or filter. All commands
were locked/offline. These component selections are not a full host regression.

Six new remap controls cover digest rejection, existing-publication protection,
pre-cancellation, exact endpoints and one-pixel axes, independent constant and
affine oracles, bad extents/nonfinite inputs, dimension bounds and execution
identity separation. One new memory control checks truthful host attribution
and rejects false overlapping peak addition. No existing assertion changed.

The first actual run of this explicitly changed mode completed all original
32 input pairs in order, with the original 60-second request wait and no retry,
skipping, warmup deletion or cache reuse. All complete output Float32 bytes and
SHA-256 hashes match the sealed historical cold outputs exactly, including the
five inputs not completed by the failed Python-mode run. A separate read-only
consumer compared actual file bytes after consuming the terminal tool result.

The driver took 81.360 seconds. Request worker time was median 2.17184695 s,
nearest-rank p95 2.3812854 s and maximum 11.9704003 s, including first-use work.
Host remap median was 0.07085015 s, p95 0.0985496 s and maximum 0.1121768 s.
One native worker PID served all 32 requests and its owned workspace was removed
on completion. Default-level logs contain exactly 32 `phase=remap_rust start`
signals, no Python remap start, and protocol stdout is empty. Historical cold
timings are not a controlled contemporary speedup comparison.

## Evidence and remaining limits

The [manifest](data/depth75-rust-remap-v1.json) and
[archive](data/depth75-rust-remap-v1.zip) contain 22 authored source, controller,
consumer, receipt and raw-log members: 56,741 bytes, SHA-256
`cc48e37dfc76a07cf0d4ad6f603f19e96656ec5b1bdd0c08188d26e83d58bb27`.
All member CRCs, lengths, hashes and bytes were read back. No model, runtime,
executable, RGB, output tensor or cache payload is distributed.

The new mode still needs actual persistent-cache isolation/reopen and live
Session/frozen-render controls. Prior Python-mode controls do not prove those
new-mode boundaries. Whole concurrent memory admission, sustained/thermal load,
accepted cross-quality transitions and full current-source regression/CI remain
pending. The earlier native crashes and broad regression failures remain
unresolved. No Android, UI or panel changes are included. #75 remains incomplete.
