# Bounded native prerender worker (#82)

Status: worker and six controls authored; compilation/execution are pending.
This is a backend owner component, not an integrated Engine/Session feature.
The background-prerender capability remains disabled; Android is excluded.

The worker creates one background thread, one command slot and one completion
slot. Preparation opens no GPU device; the independent native renderer is
initialized lazily on the worker. A prepared native-output profile must be
acknowledged before rendering. Busy or CPU-quota holds are normal None results.
Only an actual matching completion releases the caller's active job slot.

CPU admission is reserved before a render is sent. Its private permit follows
the real completion owner through mailbox drain and caller-held frames. The
frame exposes only borrowed pixels, with no into-image escape that can release
admission early. A caller must retain an old worker owner until its thread and
outstanding completion permits have retired before creating a replacement.

A Send+Sync read-only Observation handle publishes ticket, operation, frame,
phase and elapsed duration before initialization/decode/draw/readback. Watchdogs
can query it without sending a command through either serial queue. Non-Present
starts are Info; completion timing is Debug; faults are Error. Historical causes
retain at most 2048 characters. Normal cancellation is not an Error. A failed
cleanup does not replace the native producer's original failure.

Caller-supplied cancellation controls can be published before enqueue. Late
cancellation after worker publication still discards the frame on owner poll.
Shutdown sends cancellation and closes the command channel without joining a
running native owner. The worker releases its resources after actual operation
acknowledgment. A panic preserves the process hook, publishes failure and retires
that native owner; there is no automatic thread restart or native-owner reuse.

Six authored controls cover CPU ownership/quota, cancellation before enqueue
and cross-thread observation, shutdown returning before a controlled Freeze is
released, actual native pixels after Session closure, controlled blocked Draw
publication/cancellation/recovery, and an actual worker panic with no owner reuse.
These are not passing counts until the source-bound controller finishes. Existing
cache/producer controls are retained. No UI, dependency or asset is changed.

## Remaining before runtime acceptance

- Bind independent device creation to the actual foreground adapter, not merely
  another HighPerformance request that may select a different physical GPU
- Move owner-side source opening into worker preparation, leaving a pure metadata
  snapshot on the Session owner; verify a missing-source refusal at prepare
- Connect Engine foreground interruption, directional scheduler and actual
  admission, bounded completion install and Session cache consumption
- Match real preview quality, observer, media and color/output profiles; validate
  edits, source changes, document/device generations and pressure
- Execute the combined regression after aligning to main, actual default file/
  Error stderr evidence and paired foreground playback/seek P95

The preceding cache/producer/upload receipts accept their recorded sources only;
they do not accept this next component or changes brought in by a later rebase.
