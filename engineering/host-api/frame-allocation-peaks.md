# Frame allocation windows

Issue #61 needs per-frame combined peaks as well as current admission and the
budget's lifetime high-water mark. `AllocationBudget::begin_frame(request,
operation)` opens a bounded observation window on the existing shared admission
gauge. It does not allocate another native allowance or reserve extra bytes.

## Scope

The snapshot version is 2 and its scope is
`session_enrolled_native_capacity`. It observes the logical capacity of every
allocation enrolled in that budget while a window is active. Existing retained
resources are included at entry; successful reservations update every overlapping
window before native resource creation. Aliases retain one reservation. Refused
reservations do not increase the window's peak. A later window starts from the
current gauge, independently of the lifetime high-water mark.

This is combined session usage during an observed interval, not the bytes owned
exclusively by one request, physical VRAM, driver residency, or GPU hardware
timestamps. Other renderer/export work sharing the allowance can increase the
peak during an overlap. `completeCoverage` and `isPhysicalVram` remain false.
Presenter/UI resources, raw external allocations, driver/decoder memory and
implicit queue-owned staging still have separate or unknown scopes. Per-category
frame peaks are not supplied by this API.

## Lifecycle

Start before entering a potentially blocking frame operation. Finish the CPU
scope with `Rendered`, `Skipped`, or `Failed`, preserving the operation's original
return value and cause. Normal export cancellation is `Cancelled`, not `Failed`.
Dropping an unfinished scope marks `Panicked` during
unwind, otherwise `Abandoned`. Normal `Ok(false)` presentation is `Skipped`;
its existing degradation observations remain responsible for sustained faults.

Allocated command submission and `Renderer::submit_tracked` attach each submission
made while the CPU scope is open. CPU completion alone cannot retire a window
whose registered submissions are outstanding. Each queue callback releases its
upload destinations and command reservations before reporting completion in the
same callback. It does not rely on ordering between independent callbacks.
This records observed logical retirement, not a certification of GPU/device
health. `Rendered` describes the CPU operation's successful result; device errors
retain their existing reporting paths, independently of a queue-done callback.
Later submissions do not renew a CPU-ended window. These observations own only
metadata and never prolong a resource lease or block on GPU completion.

The end gauge is sampled after the registered completion callbacks and CPU
completion have joined. Elapsed time includes callback observation latency; it
is not hardware execution time. Callback destruction or submission unwind is
recorded as an unconfirmed submission, not successful GPU completion. Cleanup
never logs while a Session may be locked inside `device.poll`. A poisoned
admission mutex remains an explicit query error rather than a fabricated zero.

Direct external `queue.submit`, final upload teardown, and queues without this
budget are not automatically attached. Automatic windows cover desktop Present,
CaptureUi and NodeCapture, frozen full-resolution PNG, and frozen movie exports.
Legacy direct host capture, Presenter/UI enrollment, per-category peaks and
combined resize/seek/project-switch acceptance remain pending.

## Independent exports

`begin_frame_at(request, operation, source_frame)` additionally records the finite
authored composition frame. Fractional PNG captures retain the frozen frame,
even if the editor seeks while the export worker is blocked. Existing
`begin_frame` callers have `sourceFrame: null`. A movie `ExportJob` window starts
before encoder/native renderer initialization, covers all output frames, final
verification and publication, and has no individual source frame. Each movie
`Export` window starts before sampling/decoding and includes capture/readback,
that frame's encoding/audio work and its progress callback. PNG's `Export`
window starts before source freezing and native initialization and includes
encoding, verification and publication. CPU source-copy bytes, codec/decoder
memory and native driver overhead remain outside this admitted-capacity gauge.

Immutable movie/PNG plans carry the originating engine request across worker
handoff. Movie freezing retains that request for a later export, including after
the engine closes. Standalone callers without such a request use zero, not a
fabricated engine/session identity. Frozen dimensions, sampling, precision and
full-resolution rendering remain unchanged; observation never applies preview
quality, retries an export, changes undo or replaces a failed destination.

Metadata saturation skips the new observation but still performs the original
export. Actual observation errors are logged once per episode without replacing
the primary operation result. The export observation site has one process-wide
consecutive omission counter across PNG, movie jobs and frames: Debug at 1,
Error once at 60, reset on a successful window. Observation fault suppression
resets on a successful begin result. Logging happens outside Session/admission
locks and includes request, operation, source frame, phase and elapsed time.
The existing file/Error-stderr policy remains responsible for output channels;
healthy frames add no file I/O. Unwinding preserves the original panic. A panic
already converted to an error by the existing PNG boundary is observed as its
returned failure; the panic hook separately retains the original stack.

## Readout and bounds

`AllocationBudget::frame_peaks()` and desktop
`Engine::allocation_frame_peaks()` read the same state without a worker request,
GPU polling or file I/O. Active records publish request/operation, starting
allowance/capacity, peak, CPU outcome/timing, and pending/total submissions.
Completed records also contain the end gauge and observed elapsed time.

At most 64 active and 64 recently completed windows are retained. Saturation
increments the budget's saturating lifetime `omittedFrames` counter and skips
observation without refusing the product operation. Recent records are ordered
by observed completion, not request order. `unconfirmedSubmissions` is also a
saturating lifetime counter; record-level counters explain retained episodes.
Neither history nor omission counters are reset by querying them.

The desktop observation site logs Debug on its first consecutive omission and
Error once at omission 60, resetting on a successfully observed window. This
counter is process-wide across frame operations; it is separate from each
budget's lifetime omission count. Actual observation errors emit Error once per
episode and reset on a successful begin result. Both happen outside Session and
admission locks. Healthy Present has no added start/done/file logging. Request
IDs, a finite optional frame and an enum bound retained context; project text and
user payloads are absent.

## Validation status

The initial local CPU validation build exhausted E: before tests executed; its
stale shell exit code is rejected as acceptance. Subsequent exact-source base
`8a2cf966` and export `5efb018f` matrices passed, including actual GPU, controlled
blocking, application-feature PNG/movie, default output-channel and broad
regression gates. Commands, counts, existing ignores, immutable logs and source
boundaries are recorded in
`engineering/validation/frame-export-allocation-windows-completed.md`.

Those results do not accept the later integrated pressure, category, lifecycle
or release-scope increments. Their current candidate still requires cumulative
source-bound execution, tracked in `engineering/validation/frame-memory-admission.md`.
No new Linux/Metal runtime or complete issue coverage is claimed here.
## Resource categories

`AllocationBudget::info().categories` reports current and independent lifetime
high-water marks for 19 fixed allocation purposes. Each frame record additionally
contains `categories` with that interval's start, peak and optional end bytes.
The category end remains null until the CPU scope and every registered queue
callback have ended, exactly like the combined interval. Categories with zero
capacity remain present, making bounded snapshots explicit and predictable.

The purposes cover static images, video, vector resources, effects, masks,
compositing, preview/capture/exposure targets, authoring inputs, plugin resources,
system textures and seven buffer roles. Texture creation explicitly supplies
the purpose; it is never inferred from a user-visible label. Buffer roles use
descriptor usage, with mapped readback/upload, query resolution, geometry,
storage and uniforms taking precedence over the explicit other-buffer bucket.
The existing generic offscreen target API retains its capture-target purpose;
internal preview, nested-composition and exposure callers select their purpose.

A reservation receives one immutable category before native creation. Texture,
view, binding and buffer aliases and queued/upload retentions do not duplicate
or move that reservation. Refused admission changes neither the combined nor
category counters. Checked subtraction detects category drift and stops normal
admission/readout instead of silently clamping a counter to zero. No logger is
invoked while holding admission state.

Category maxima usually occur at different moments. Their sum is not the
simultaneous frame peak: only `peakAdmittedBytes` on the frame expresses that
combined peak. At healthy current/end snapshots, category current/end bytes sum
to the same combined gauge. No driver, swapchain, raw external allocation or
CPU decoder capacity is fabricated into a category. `completeCoverage` remains
false until the remaining application ownership boundaries are enrolled.

## Release-fault observation scopes

Hosts that release enrolled resources while holding business guards must bind an
`AllocationLogScope::enter(session)` before acquiring those guards. Motion Host
does this in `with_session` and `close`. The scope stays on its originating thread;
nested scopes flush only when the outer scope drops, including panic unwind.
The budget mutex and the thread-local observation borrow are both released
before the logger runs. Standalone render callers without a business guard keep
the immediate Error path and do not need a scope.

The existing invariant failure remains once per budget lifetime and stops further
admission/readout. Deferral retains one fixed-size first cause per thread and a
saturating count of additional budget faults until the outer operation returns.
It includes the original allocation ID, category, bytes, flags and session,
retirement phase and deferred duration. It neither retries nor changes the
primary caller result or panic. A new operation starts with empty pending state.

Source-bound base and export acceptance is recorded in
`engineering/validation/frame-export-allocation-windows-completed.md`.
Pressure, category, lifecycle and release-scope increments still require their
own cumulative-source results; earlier passing totals do not validate them.
