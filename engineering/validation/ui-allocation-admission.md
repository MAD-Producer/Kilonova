# UI native allocation admission (issue #61)

Status: implementation candidate; compilation and actual runtime acceptance are
pending. This is a functional budget increment, including upload recovery; it
is not merely an observation change and is not issue completion evidence.

## Scope

Desktop attachment, presentation and UI capture construct an admitted Painter
from the same renderer AllocationContext. Uniform and geometry buffers use the
existing usage categories, and glyph textures use the new fixed `ui_atlas`
purpose. The category count becomes 20. There is no new third-party package or
asset: motion-ui adds a path dependency on the existing motion-render crate,
whose native context owns admission and upload retirement. The locked offline
dependency graph was checked, but that is not compilation acceptance.

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
PaintError::source and typed resource_rejection; vertex/draw limit fields and
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

## Required acceptance (not yet executed)

| ID | Actual guard | State |
| --- | --- | --- |
| U1 | `startup_refusal_is_atomic_and_actual_ui_allocations_have_disjoint_categories`: real native constructor refusal, partial cleanup, typed cause, uniform32/geometry8/atlas16 category deltas and same-texture write reuse | Pending |
| U2 | `refused_atlas_and_two_buffer_growth_preserve_old_pixels_then_recover`: native atlas replacement and second buffer refusal, complete old state/pixels, recovery, one pending owned upload,60 capacity checks without repeated allocation, subsequent upload/pixel recovery | Pending |
| U3 | `dropped_painter_remains_in_frame_peak_until_real_gpu_work_retires`: real draws/uploads, dropped owner before submission, retained capacity, GPU completion, reconciled current bytes and atlas frame peak/end | Pending |
| U4 | `admitted_text_matches_legacy_pixels_through_real_atlas_growth_and_multiple_dpis`: actual glyph rendering, clipped text and real atlas growth at 1x/1.5x/2x DPI; admitted pixels must exactly match the legacy path | Pending |
| U5 | Application `native_ui_refusals_preserve_cause_and_reach_default_file_stderr_without_stdout`: actual native startup/atlas failures, original host failure precedence, real reply, default Info file and Error stderr, stdout isolation | Pending |
| U6 | Workspace/all-target FFmpeg and no-FFmpeg compilation, full motion-ui/render/host/media/desktop suites, existing DPI/clipped-text/atlas-growth/pixel guards with assertions unchanged, native lifecycle 24-case artifacts, style guard | Pending |

Existing UI test fixtures now consume the fallible atlas upload result; their
assertions are unchanged. Example render/capture functions propagate the cause.
There is no panel, palette, token, layout, text, glyph geometry, shader or
interaction change. Presenter and Session files owned by the other active
worktree are untouched. Presenter allocation coverage remains separate work.

All temporary/build/test artifacts and raw exact-source/hash/exit evidence belong
on E:. Earlier pressure/base/export results do not validate this candidate.
