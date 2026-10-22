# Frame allocation windows

Issue #61 needs per-frame combined peaks as well as current admission and the
budget's lifetime high-water mark. `AllocationBudget::begin_frame(request,
operation)` opens a bounded observation window on the existing shared admission
gauge. It does not allocate another native allowance or reserve extra bytes.

## Scope

The snapshot version is 1 and its scope is
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
return value and cause. Dropping an unfinished scope marks `Panicked` during
unwind, otherwise `Abandoned`. Normal `Ok(false)` presentation is `Skipped`;
its existing degradation observations remain responsible for sustained faults.

Allocated command submission and `Renderer::submit_tracked` attach each submission
made while the CPU scope is open. CPU completion alone cannot retire a window
whose registered submissions are outstanding. Each queue callback releases its
upload destinations and command reservations before reporting completion in the
same callback. It does not rely on ordering between independent callbacks.
Later submissions do not renew a CPU-ended window. These observations own only
metadata and never prolong a resource lease or block on GPU completion.

The end gauge is sampled after the registered completion callbacks and CPU
completion have joined. Elapsed time includes callback observation latency; it
is not hardware execution time. Callback destruction or submission unwind is
recorded as an unconfirmed submission, not successful GPU completion. Cleanup
never logs while a Session may be locked inside `device.poll`. A poisoned
admission mutex remains an explicit query error rather than a fabricated zero.

Direct external `queue.submit`, final upload teardown, and queues without this
budget are not automatically attached. Render/export integrations can use the
public scope API, but automatic windows in this increment cover desktop Present,
CaptureUi and NodeCapture only. PNG/movie frame integrations and combined
resize/seek/project-switch acceptance remain pending.

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
IDs and an enum bound retained context; project text and user payloads are absent.

## Validation status

The initial local CPU validation build exhausted E: before tests executed.
Its shell wrapper returned a stale exit code after an output/launch failure;
the compiler's `no space on device` output is authoritative. It is not passing
acceptance. New CPU, native GPU and actual worker assertions remain pending
until exact-source validation completes. No Windows/Linux/Metal runtime or
complete issue coverage is claimed by this document.
