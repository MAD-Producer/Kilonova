# Frame and export allocation windows: accepted increments

For later integrated Presenter/UI, CPU decode, pressure/category/lifecycle
and release-scope results, see [cumulative acceptance](resource-budget-preview-completed.md). The exact-source base/export results
below remain historical acceptance of their own increments.

This document accepts the bounded observation increments below, not issue #61 as
a whole. Logical admitted capacity is not physical VRAM or complete application
memory. Implementation requirements and remaining limits are traced in
`frame-memory-admission.md` (#61-A130 through #61-A142).

## Exact-source acceptance

| Source | Accepted behavior | Runtime / build evidence |
| --- | --- | --- |
| `8a2cf966395937f65c33a99524d210df7d9fe9ac` | Shared frame intervals start before Session entry; transient peaks, bounded overlap/history, CPU/queue completion join and nonqueued Engine readout | Nine phases passed: CPU 6, actual GPU 2, helper 3, blocked worker 1, full render 447, full host 120, full desktop 756, workspace checking and style guard |
| `5efb018fb14a0b517952311788792a1845e4a6fa` | Independent frozen PNG/movie job/frame windows preserve real request and authored time across freeze, cancellation and Engine exit | Twelve phases passed, including real PNG 4, movie 2, desktop PNG 6, application movie 7 and full application-FFmpeg desktop 761 |
| Same export source, no default features | PNG remains usable without FFmpeg | Desktop/media all-target check and four actual PNG tests passed; refreshed source-switch validation also passed |

Focused counts overlap full suites and must not be added to their totals. The
full render suite retains one pre-existing optional local-image ignore; both
desktop totals retain the existing native audio-device ignore. No assertion was
weakened and no skip or ignore was added. The host total includes integration
tests; its library subset alone is not the full count.

The export matrix explicitly enables **application** `motion-desktop/ffmpeg`.
An earlier desktop run enabling only the media dependency's FFmpeg feature is
not coverage of application tests behind `cfg(feature = "ffmpeg")`.

Raw immutable logs and subprocess exit/source/SHA-256 receipts are retained at:

- `artifacts/pr-queue/frame-allocation-peaks-final-v1-*`
- `artifacts/pr-queue/export-allocation-windows-final-v1-*`
- `artifacts/pr-queue/preview-pressure-refreshed-v3-export-default-*`

The local `frame-allocation-peaks-acceptance.json` and
`export-allocation-windows-acceptance.json` reverify the corresponding raw hashes.
Compiler success, empty test filters and a surrounding shell's stale exit code
are not runtime acceptance. A recorded exploratory disk exhaustion is rejected;
its failure log remains available rather than being relabelled as a pass.

## Observations and preserved boundaries

Start windows and existing serial-worker in-flight publication precede handlers.
Completion joins CPU return with all registered allocation queue callbacks;
cleanup runs before the final gauge observation. Metadata limits omit observation
without refusing actual allocation or changing cancellation/publication results.
Controlled native saturation, primary failure/unwind and recovery guards exercise
these distinctions. Actual default-level logger tests verify file output, Error
stderr and stdout isolation, including threshold-60 suppression and reset.

Formal PNG/movie rendering keeps full authored quality and the frozen project's
sampling/pixel semantics. No preview quality floor is reused by frozen exports.
The original operation result remains primary if observation is unavailable.
Protocol-defined discarded replies remain unchanged. No panel, layout, style,
shader, effect enablement, authored parameter or history behavior was changed by
these two observation increments.

## Work still requiring independent acceptance

Native pressure consumption, per-category peaks and repeated native lifecycle
tests are later local increments with separate sources and validation. Their
implementation cannot inherit the passing results above. Pressure source
`d92f8e9a` currently has passing focused policy/probe/lock/native-GPU tests; broader
regression remains in progress. Its GPU fixture explicitly uses an interior
frame because a centered shutter at frame zero legitimately samples before the
clip. The original opaque-color and formal-pixel assertions remain intact.

At the accepted base/export sources, Presenter/UI allocations were still outside
the enrolled gauge. Later UI and Presenter enrollment is recorded separately in
`ui-allocation-admission.md` and `presenter-buffer-admission.md`; the receipts in
this document do not accept those later implementations. Raw external submissions/resources, driver
staging/swapchain and CPU decoder/source-copy capacity are not made complete by
these logical windows. No new Linux/Metal or stopped Android device acceptance
is claimed. Issue #61 and the related #60/#67 remain open; no 3D readiness or
whole-application memory bound is declared.
