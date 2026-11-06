# Current desktop depth verification — issue #74

## Source and scope

The frozen product source is `4cd60b0f36e7e86d549af9909ad8a26d5dc7e967`.
This checkpoint re-executes the optional depth contracts and real video
Session/FFmpeg/ONNX path on the current combined branch. It changes no product
code, panel, style, assertion, precision policy or dependency. Android is retired
and excluded. This is a targeted verification, not a complete workspace gate.

The external probe uses current workspace path dependencies and the current
desktop file/stderr logger. Its 304-package lockfile contains no registry
version or checksum absent from the workspace's 571-package lockfile. Build
and runtime receipts freeze the product files, lockfile and private probe
sources. All temporary files, build outputs and inference scratch stay on E:.

## Newly executed results

| Gate | Result | Boundary |
| --- | --- | --- |
| Complete `depth_jobs` target | 8 passed | Controlled blocking, cancellation, replacement, installation identity, budgets and terminal backend panic |
| Complete `depth_session` target, actual GPU enabled | 17 passed | Image ownership, calibration, history/reopen, stale installation, frozen captures and actual rendering controls |
| Complete `depth_video` target, actual GPU enabled | 22 passed | VFR identity, source preparation, colour conversion, indexed spill, invalidation and actual rendering controls |
| Complete ONNX unit module | 4 passed | Existing backend protocol contracts |
| Actual native Session/FFmpeg/CPU ONNX | Passed | Current native video preparation and inference, not the GUI Engine or rendering of this inferred result |
| Requested unavailable CUDA provider | Passed with actual CPU fallback | Real ONNX Runtime and model execution, not simulated provider replies or accelerated CUDA execution |

All 51 Rust tests passed with zero failures and zero ignores; the controller
completed in 124.235 seconds including compilation. The earlier CPU-only
invocation passed 39 tests but left eight existing GPU tests feature-gated.
Its controller correctly stopped at the unexpected ignore inventory. That
original output is retained; the subsequent invocation explicitly enables
`ai-depth,gpu-tests` and executes all eight controls. No ignore attribute or
assertion was changed.

## Actual inference, colour and failure evidence

The authored, tagged BT.709 VFR fixture has a nonzero start. A request at
76,667 us resolves to the actual decoded source PTS of 50,000 us. Explicit
crop, clockwise rotation and horizontal mirroring produce a 52-by-80 RGB
input and a 52-by-80 R32Float relative-depth resource. The prepared RGB bytes
match the previously independently accepted sRGB input exactly. Unknown
colour metadata is rejected before entering the backend.

The current native CPU run passes the original normalized maximum error
bound `2e-4` and mean bound `2e-5`: observed maximum
`7.787024532934269e-7`, mean `1.2020595162418164e-7`. The reference is the
retained independent official PyTorch output from source `06d1c716`, verified
by SHA-256 `2695b67c87473b1d89fb56763d24ce87836983c5082a77ef5a72f0d4858eb546`.
This is a fixed-golden comparison, **not** a fresh PyTorch execution. Model
hash, reference receipt, input bytes and unchanged numerical budgets are
verified before accepting that comparison. The source and relative-depth
visualizations were inspected; this small synthetic fixture adds no natural
scene-quality evidence.

The actual missing-index failure precedes backend entry. Explicitly restoring
the verified index permits a new request; no retry is added to the product.
Seek invalidates the installed result, the project remains unchanged, and
temporary inference files are cleaned up. With `MOTION_LOG` unset, Info start
precedes failure, Error reaches both the real file sink and stderr, and product
stdout stays empty. The external diagnostic probe's phase markers are separate
from product logging.

## Actual unavailable-provider fallback and costs

ONNX Runtime `1.31.0` reports available providers
`AzureExecutionProvider` and `CPUExecutionProvider`. CUDA is absent. A fresh
native Session request explicitly asks for CUDA; actual profiled model
execution reports `CPUExecutionProvider`, and the default-Info file log records
`requested_provider=CUDAExecutionProvider cpu_fallback=true`. Its prepared
input and numerical output pass the same fixed-golden checks.

Measured costs from that single execution, without a performance claim:

| Measurement | Observed value |
| --- | --- |
| Source RGB preparation | 3.9583 ms, 12,480 bytes |
| Model/runtime load | 529.7323 ms |
| Preprocessing | 18.7934 ms |
| Host input copy | 0.7100 ms, 3,219,888 bytes |
| Actual inference | 961.8546 ms |
| Host output copy | 0.2847 ms, 1,073,296 bytes |
| Postprocessing | 1.1949 ms |
| Parent host transport | 9.2662 ms |
| Backend total, including child/cleanup overhead | 1,690.9166 ms |
| Session total | 1,696.5117 ms |

Provider-internal transfers are included in inference time. These measurements
do not establish I/O binding, zero-copy transport, GPU-provider support or a
performance baseline.

## Retained failures and limits

The initial private debug probe overflows its Windows process-main stack with
exit `0xc00000fd`. A second unchanged-boundary probe adds diagnostic phase
markers and locates the observed failure inside `Session::new`; both raw
failures remain in the archive. The measured Session size is 81,136 bytes,
which alone does not establish the constructor's total stack requirement.

The final private probe places Session ownership on a default Rust worker
thread, matching the desktop engine's ownership boundary. It sets no custom
stack size, linker flag, backend, timeout or retry. This boundary passes; it
does **not** repair or erase the process-main failure or prove that every
caller has sufficient stack. The external probe remains distinct from the
actual GUI Engine. The unrelated retained full-regression access violation
`0xc0000005` is also unresolved; these observations do not establish a common
cause.

The first build controller stopped before compilation because it attempted to
hash an SDK script excluded by this sparse worktree. Its original controller
is preserved. The corrected closure covers checked-out product crates, desktop
sources, manifests and lockfiles, plus all private probe inputs.

Historical portrait, illustration, natural interior and real-video quality
records retain their original source scope. This execution does not repeat
that domain evaluation, prove calibrated physical depth, run Android, exercise
an available accelerated provider, render the newly inferred ONNX result in
the GUI, or replace complete regression and final-head cross-platform CI.
Issue #74 and the full 3D route are not closed by this checkpoint.

## Evidence

[Source-bound commands, probes, failures, original output and receipts](data/depth74-current-4cd60b0f.zip):
4,202,242 bytes, 170 members, SHA-256
`9fd6389f1c69c646fb356ef6b5e52c824ba147c4b335f4a748ec5b7f5634091a`.
Every member was read back and verified; the adjacent JSON contains the member
manifest. The archive includes the pinned reference output and prior reference
receipts, but no model weights, runtime binaries, executables or third-party
source trees. The media fixture is independently authored.
