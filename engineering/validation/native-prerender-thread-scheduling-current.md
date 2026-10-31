# Optional native owner thread scheduling

Issue [#82](https://github.com/MAD-Producer/Kilonova/issues/82), requirement S3.
This is a scheduling implementation and Windows component record, not a claim
that the foreground P95 acceptance requirement has passed.

The dedicated native prerender owner previously inherited ordinary OS thread
priority. Cooperative cancellation did not change CPU scheduling while native
initialization, rendering or teardown was still executing. Optional work now
lowers only that owner thread before source preparation or native construction:

- Windows: `SetThreadPriority` with `THREAD_PRIORITY_LOWEST`, preserving an
  inherited still lower priority
- Linux: `setpriority(PRIO_PROCESS, 0, 10)`, preserving a higher inherited nice
  value; Linux applies this operation to the calling thread
- macOS: `pthread_set_qos_class_self_np(QOS_CLASS_BACKGROUND, 0)`

The OS policy is read back. An unavailable or ineffective policy returns the
original OS cause through the existing failed-completion path. A worker caches
that result, including failure, rather than retrying per frame or quietly doing
optional work at normal priority. Existing default-level worker start precedes
the call; in-flight phase `scheduling` covers it. Existing Error logging and
observation retain the failure without holding an observation lock while logging.

No foreground thread or process priority is changed. CPU scheduling does not
set GPU queue priority, cancel already-submitted GPU work, impose thread timeouts
or bound a defective driver's duration. The lowered policy remains on this
dedicated thread during cleanup. Foreground rendering and export use their
existing owners. No dependency, UI or Android scope is added.

The independently authored tests read real OS priority in a new thread, check
that applying the policy twice is safe and that the parent remains unchanged.
A separate controlled real Worker checks the policy at its `freeze` phase,
before source preparation. Its original-production negative control must fail
on the actual priority assertion, followed by the same unchanged test with the
production change.

The Windows gate completed at `2026-10-31T06:30:44.368462+00:00`. Without the
production scheduling call, the unchanged real-worker test failed with actual
foreground priority 0 (exit 101, zero passed and one failed). With that call,
the same test passed. The separate actual OS isolation/idempotence test passed;
the native-worker suite passed 18 tests with zero failures and two existing
ignored studies. Focused tests overlap that suite, not additional unique tests.
Provenance, inventory and whitespace checks passed. Cross-platform execution,
final integrated regression and strict input P95 remain pending. This does not
diagnose or fix historical native crashes.

The [source-bound raw negative/positive controls and worker regression](data/native-prerender-thread-scheduling-v1.zip)
retain all actual child exits and original outputs, including the intended
negative assertion failure. No failure was converted to a successful command.

Public API references (behavior only; no upstream source or assets copied):
[Microsoft SetThreadPriority](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setthreadpriority),
[Linux getpriority/setpriority](https://man7.org/linux/man-pages/man2/setpriority.2.html),
[Apple pthread QoS](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/EnergyGuide-iOS/PrioritizeWorkWithQoS.html).
