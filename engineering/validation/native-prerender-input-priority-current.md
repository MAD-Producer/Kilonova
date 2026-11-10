# Native input admission before action generation (#82)

Component only. Source parent `e6110e800c411a9bf5a941cde1c0da3ea48a34ae`, based on main
`ab779cee3c7f61f46ae8dad7b4b5e331b7298ceb`; the receipt records every changed
source hash and the exact negative/fixed commands. Android is retired.
This does not close #82 or accept whole-application P95 or cross-platform CI.

The native main and floating window event entry publishes input priority before
drag routers, hit testing and action generation. This short signal neither
queues a command nor performs IO. A bounded three-button mask and checked epoch
close optional publication/admission during a gesture. Release does not itself
reopen it: only a successful nonfrozen composition Present queued with the
current input epoch can acknowledge it. Held buttons and older queued frames
cannot acknowledge newer input. The observer itself requests native redraws so
inputs ignored by the UI translator cannot strand the acknowledgment gate.
Ordinary hover remains quiet. Keyboard presses,
wheel, IME composition and file drops publish pulses; focus loss cancels held
buttons. Hidden composition views conservatively wait for their next successful
composition Present. Existing reconciliation/cancellation acknowledgment runs
before the input admission hold, so the gate does not strand canceled work.

No panel, layout, style, dependency, editor action or return value changed.
The new failure observation uses an existing bounded fixed-site counter, resets
on success, and logs outside the signal lock. Healthy input/Present adds no IO
or log records. State exposes inputPending and the input hold reason.

## Actual native negative and fixed proof

The actual Shell window-event path receives an injected mouse event while its
Engine is blocked in TestWait. A fresh production YieldControl is published
AFTER TestWait dispatch, so the dispatch's ordinary cancellation cannot satisfy
the assertion. Hover must leave it live; mouse press must cancel it before any
new command is enqueued, while in_flight remains test_wait and queue_depth is0.
This first phase is a controlled cancellation-token proof, not a claim that the
blocked owner is rendering. The second phase uses a genuine native producer:
warm two frames, hold admission while the button is pressed, retain that hold
after presentation while pressed, release, successfully present, then require
new completed native warmup work.

Removing ONLY the native event-entry hook made the unmodified test fail at the
pre-action-generation cancellation assertion: child0/1/0 and parent0/1/0.
Independently removing ONLY the observer's redraw request failed the native
observer-only acknowledgment redraw assertion: child0/1/0 and parent0/1/0.
Restoring both exact entry points passed1/0/0. The fixture receives an actual
native RedrawRequested after direct observer input without UI translation, then
successfully presents before accepting admission recovery.
The fixed fixture recorded started
2 while held/still held and 4
after actual foreground acknowledgment and subsequent warmup. Two pure input
protocol tests and all five signal tests passed; these separately guard stale
frame acknowledgment, held/multiple buttons, focus cancellation, invalid button
indices and publication races. Tool inventory and provenance passed.

The initial controller was stopped while still waiting behind the full gate so
the redraw liveness omission could be corrected before validation began. It
executed zero tests; its original controller and interrupted-wait receipt are
retained. No native failure or successful run was discarded or rerun unchanged.

## Limits and retained failures

The input-v3 study on the parent source actually completed480 samples but FAILED
the unchanged P95 criteria: property pooled1.097659032273382 and timeline
pooled1.122692849259809. The parent's combined-v4 playback first pair also
FAILED1.0508877615726062 despite pooled0.8776137761377614. No sample, assertion,
pair criterion or threshold was relaxed. These component controls do not prove
that the new admission gate resolves either performance regression.

Automatic same-path external source invalidation, final-source complete desktop
regression, unchanged input/playback P95 and cross-platform CI remain pending.
Original input-v2 heap corruption and earlier full-gate access violation remain
unresolved; neither this success nor later crash-free execution proves causality.
No optional disk cache, disk-full/corruption or physical low-memory qualification
is claimed.

Archive: `data/native-prerender-input-priority-v1.zip`.
SHA-256: `b0f7325f43a35bfe101662b5e4413a66ae32a6851676b78cf5d62099d8cc89c5`. All32 members were read back byte for byte.
No executable, PDB, native dump or user project is included.
