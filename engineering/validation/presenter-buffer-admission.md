# Presenter uniform admission (issue #61)

Status: implementation candidate, not yet compiled or accepted. The earlier
source-bound UI/CPU results remain evidence only for their recorded sources.
This document does not close issue #61 or claim a complete process/VRAM bound.

## Implementation

`Presenter::new_admitted` now returns a native Result and admits each 32-byte
uniform before its native buffer creation. Refusal of the second buffer releases
the first reservation and preserves the typed original cause. The 64 bytes are
part of the existing fixed `uniform_buffers` purpose and combined frame peak;
they are not a new allowance. The source view keeps its existing reservation.

The navigation update records its destination in the originating renderer's
upload journal before queue staging. It uses that renderer's queue, and retains
the 32-byte destination until real submission completion or the journal's
existing final teardown flush. Allocated encoders retain both buffers and the
bound source across owner destruction. Bind groups drop before their buffer and
source reservations. Presenter remains Send; no thread-local context is placed
inside Session, and no new timeout, retry or restart is introduced.

Standalone `Presenter::new` retains its explicit raw resource path. Native
Session attachment and desktop capture use the fallible admitted constructor.
Navigation encode now returns native errors to ordinary, exposure, retained and
capture callers. The ordinary preview path releases an unsubmitted timestamp
slot if the upload fails. Healthy encoding, UVs, color conversion, source binding
policy and pixels remain unchanged.

Session prepares every replacement resource before detaching its existing
graphics. A failed replacement thus retains the previous valid front and epoch.
Successful replacement still uses the existing detach/poll sequence. This is a
functional resource-failure fix, not merely additional logging.

The desktop worker publishes its SurfaceFactory and GPU diagnostics only after
Session accepts the replacement. A refused renderer, Painter or Presenter leaves
the existing factory intact. Publication also occurs if snapshot serialization
fails after a successful attachment. Factory locks do not span session/GPU work,
and replaced handles drop after unlocking. Poison remains an error; a snapshot
failure stays primary if publication also fails. This keeps future floating
surfaces on the accepted device rather than a rejected replacement device.

The separate source-binding reuse worktree is untouched. Only its branch/status
and untracked filename were inspected to distinguish that optimization from
this resource-admission task. No other PR is reviewed or merged.

## Required acceptance

| ID | Actual guard | State |
| --- | --- | --- |
| P1 | `native_presenter_refuses_second_uniform_atomically_then_admits_exact_capacity`: actual second-buffer refusal, original cause, rollback, exactly64bytes/two allocations in the uniform category, constructor recovery and final release | Pending |
| P2 | `actual_navigation_pixels_and_dropped_presenter_share_real_frame_retirement`: actual cropped pixels, dropped owner/source before submission, retained combined capacity, GPU completion, exact category/frame end reconciliation | Pending |
| P3 | `unsubmitted_navigation_write_outlives_presenter_without_a_command_scope`: abandon raw commands; queued destination alone remains charged32bytes until an actual allocated empty submission completes | Pending |
| P4 | `native_presenter_refusal_during_surface_replacement_preserves_front_and_recovers`: actual hidden native surfaces, refusal after replacement placeholder creation, old valid front/epoch/pixels/history retained, successful replacement after restoring budget and final native retirement | Pending |
| P4b | `native_failed_reattach_keeps_original_factory_front_and_default_error_channels`: actual serial worker and two hidden native windows; refuse the second Presenter uniform by one byte, preserve original factory/device and present old front under pressure, exact before/after capture, recover replacement, default Info file/Error stderr and stdout isolation | Pending |
| P5 | Full render/host/UI/media/desktop regressions, ordinary/exposure pressure,24-case lifecycle, actual worker atlas recovery, frozen PNG/movie, default file/stderr/stdout isolation, all-target default/FFmpeg checks and styles | Pending |

The existing strict retained-source allocation test previously budgeted only the
renderer plus its 256-byte source, because Presenter uniforms were not enrolled.
Its fixture and two exact current-gauge expectations add the actual64bytes;
the source size, lowered limit, refusal, every green pixel and final baseline
release checks stay unchanged. This accounts for newly enforced resources; no
pixel tolerance, assertion strength, test selection or ignore state is relaxed.
Other existing call-site changes only consume the new fallible results.

All build/temp/acceptance files stay on E:. No panel, token, layout, control or
interaction change, dependency, third-party asset or compatibility layer is added.
