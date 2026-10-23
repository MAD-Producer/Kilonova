# UI native allocation admission (issue #61)

Current local Windows acceptance: all 15 gates pass on `03f629938635bfb7566520deacd5acf303a0880c`; see [cumulative acceptance](resource-budget-preview-completed.md)
and its source/hash/command manifest. This includes native failure/recovery,
full suites, actual GUI/MCP, default output channels and Release checking.
Remote PR checks and new Linux/macOS runtime results are separate. The user
confirmed maintained Windows/Linux/macOS desktop delivery on 2026-10-23.
Android is unmaintained, outside this delivery, and has no build/device
acceptance. Issue #61 is not closed by this local record.

Historical status before cumulative acceptance: source c9f65122 passes all-target UI and application-FFmpeg workspace
checks, four actual UI GPU tests and the default application error-channel test.
The cumulative matrix stopped at invalid CPU decode test asset paths (1 pass,
5 failures before decoding); fixture correction 96022bdd preserves every
assertion. Source c8fb11e5 additionally passes actual worker recovery, full
motion-ui147 and workspace/default all-target checks. Its full-render run fails
at an expected mask snapshot missing newly added category history; later broad
suites did not execute. Final cumulative regression remains pending.

## Scope

Desktop attachment, presentation and UI capture construct an admitted Painter
from the same renderer AllocationContext. Uniform and geometry buffers use the
existing usage categories, and glyph textures use the new fixed `ui_atlas`
purpose. The category count becomes 20. There is no new third-party package or
asset. The toolkit defines the native allocation interface, and the desktop
host injects the renderer-owned implementation. motion-render is a dev-only
toolkit dependency; native tests compile the real desktop adapter. The unchanged
model dependency-boundary assertion and final-source native tests pass.

Admitted construction reserves 40 logical bytes for two 16-byte uniforms and
two minimal 4-byte empty vertex buffers. Actual geometry grows geometrically
under the device and shared Session bounds. This avoids charging an unused
multi-MiB legacy reservation against a small preview allowance. Standalone raw
Painter construction retains its original eager capacities and rendering path.

Both buffer growth candidates must be admitted before either old buffer is
replaced. Atlas replacement must be admitted and uploaded before its texture
and binding replace the old pair. Uploads and allocated encoders keep the same
reservation alive through real queue completion, including destruction of the
Painter before submission. Native failures keep their original RenderError via
PaintError::source; capacity_refusal publishes only typed capacity metadata.
The native source retains its exact rejection and scope; vertex/draw limit fields and
their old failure text remain. PaintError is still Clone; it no longer promises
value equality for opaque native causes.

An owned failed atlas upload retains exactly one already-created CPU bitmap and
its original failure, because the CPU producer consumed its dirty flag before
the GPU failure. Subsequent presents check capacity without a new allocation
attempt while it is too small. Available capacity permits one ordinary upload;
a newer bitmap supersedes the pending one. Success clears the pending state.
Invalid payload/device-dimension failures are not retried. The pending bitmap
is bounded by the supplied, validated device dimensions; the desktop TextAtlas
itself remains bounded to 2048x2048 RGBA (16 MiB). It is CPU memory outside native
GPU admission, not a complete process-memory bound.

## Source-bound acceptance and remaining gates

| ID | Actual guard | State |
| --- | --- | --- |
| U1 | `startup_refusal_is_atomic_and_actual_ui_allocations_have_disjoint_categories`: real native constructor refusal, partial cleanup, typed cause, uniform32/geometry8/atlas16 category deltas and same-texture write reuse | Pass `03f62993` |
| U2 | `refused_atlas_and_two_buffer_growth_preserve_old_pixels_then_recover`: native atlas replacement and second buffer refusal, complete old state/pixels, recovery, one pending owned upload,60 capacity checks without repeated allocation, subsequent upload/pixel recovery | Pass `03f62993` |
| U3 | `dropped_painter_remains_in_frame_peak_until_real_gpu_work_retires`: real draws/uploads, dropped owner before submission, retained capacity, GPU completion, reconciled current bytes and atlas frame peak/end | Pass `03f62993` |
| U4 | `admitted_text_matches_legacy_pixels_through_real_atlas_growth_and_multiple_dpis`: actual glyph rendering, clipped text and real atlas growth at 1x/1.5x/2x DPI; admitted pixels must exactly match the legacy path | Pass `03f62993` |
| U5 | Application `native_ui_refusals_preserve_cause_and_reach_default_file_stderr_without_stdout`: actual native startup/atlas failures, original host failure precedence, real reply, default Info file and Error stderr, stdout isolation | Pass `03f62993` |
| U6 | Workspace/all-target FFmpeg and no-FFmpeg compilation, full motion-ui/render/host/media/desktop suites, existing DPI/clipped-text/atlas-growth/pixel guards with assertions unchanged, native lifecycle 24-case artifacts, style guard | Pass `03f62993` |
| U7 | `actual_ui_atlas_refusal_recovers_in_worker_without_a_second_upload`: actual Windows Engine/Present, two refusals retaining old native atlas, budget recovery without a producer resend, exact native category/pixel/history checks and default output channels | Pass `03f62993` |

The same c9f65122 source passes 15 category/release guards, the actual frame
category guard, two native host pressure guards and all 24 native lifecycle cases.
The cumulative CPU failure and missing later phases are retained, not counted as
a full pass. Native UI log SHA-256:
`a22f1c3140fb7d8f9ff8e8a616d20c208610e45e043587841e09cb2a4510beab`;
application output log SHA-256:
`e2c13f909a4b06e580329c044e5686171f3aa8cca8f640b1670c18a5d6137436`.

Actual c8fb11e5 worker atlas recovery artifacts retain report/log/stdout/stderr
and before/after captures with identical image SHA-256
`f47355f1fd1e950d2808cc1830628eb5d30a2e9c0d7199559742585b872662c2`.
The full UI log SHA-256 is
`2b4575da5c784f26206fa20b7db0d05ecac6e649da30f09b775c36869c7f6f44`.

Existing UI test fixtures now consume the fallible atlas upload result; their
assertions are unchanged. Example render/capture functions propagate the cause.
There is no panel, palette, token, layout, text, glyph geometry, shader or
interaction change. The foreign source-binding reuse worktree is untouched.
The separate own Presenter/Session admission increment is described in
`presenter-buffer-admission.md`; its exact-source native/full-suite results
are included in the cumulative acceptance record.

All temporary/build/test artifacts and raw exact-source/hash/exit evidence belong
on E:. Earlier pressure/base/export results do not validate a later source;
the cumulative manifest independently verifies the current candidate.
