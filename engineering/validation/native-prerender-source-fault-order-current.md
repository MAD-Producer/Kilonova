# Native source watcher fault ordering

Issue: [82](https://github.com/MAD-Producer/Kilonova/issues/82).
Base source: `510484160fde38d18b177ba8c69baf19af1730dc`.
Component gate completed: `2026-10-30T17:11:19.290073+00:00`.
Disposition: accepted source-fault-order component only.

## Problem and change

A normal source event already cleared observation validity. A subsequent native
watcher error then returned early because validity was false, losing the terminal
fault. The coordinator could treat this as an ordinary refresh instead of
preserving its existing explicit recovery boundary.

Retain two independently bounded causes: the first invalidation cause and the
first terminal watcher fault. A terminal fault remains observable even after a
normal change, cancels the actual active control, and emits Error once at the
default Info threshold. Repeated callbacks and retired observations remain
quiet. Both retained causes are limited to 512 Unicode characters; logging and
cancellation occur after releasing the observation lock.

The coordinator prefers the retained terminal fault, closes lookup/admission
and still drains the real cancellation acknowledgment. A simultaneous primary
worker failure takes precedence; the watcher failure is secondary evidence.
No replacement worker, timeout, automatic retry or UI change is introduced.
This is a lifecycle/cause-preservation fix, in addition to its observations.

## Actual negative and fixed controls

All existing assertions and test bytes were unchanged between the original and
fixed executions. Three new controls failed against the original implementation
for their intended assertions:

1. Normal change followed by watcher fault did not preserve fault state.
2. An isolated default-Info logger saw only the normal change, instead of one
   change and one terminal Error. Its child and parent each reported one failed
   test; these are two harness records for one control, not two distinct tests.
3. A real native worker was held at draw, received a change and terminal fault,
   then acknowledged cancellation. The coordinator lost the terminal cause.
   The fixture releases the worker before asserting, including on the negative
   path, so it does not strand the actual owner.

The fixed source passed all three unchanged controls. The logger control checks
bounded multibyte context, suppression across repeated callbacks, retirement
and successful `try_lock` during emission. The native control checks actual
cancellation, acknowledgment, zero retained CPU bytes and a closed fault gate.

Additional actual results:

- Source watcher group: 10 passed, zero failed
- Native source freshness group: four passed, zero failed
- Media prerender regression: 43 passed, zero failed, one existing manual
  four-precision control ignored under its original selection
- Existing desktop default source-watch file/Error stderr/stdout control:
  one passed
- Desktop check, provenance and tool inventory: passed

These groups overlap: the three fixed controls are included in the groups and
the groups are included in the media regression. Do not add their counts.
The new fault-order emission is exercised with an isolated capture logger; the
actual default file/stderr/stdout sink is separately exercised by the existing
desktop source-watch control. No claim is made that every new event ordering
was independently replayed through that desktop sink.

The first controller version was stopped while queued, before compilation or
tests. Its verifier expected a single negative result for the isolated logging
wrapper, although a genuine child failure is also reported by the parent. The
second version correctly requires both failures; all test/source/patch bytes
and every other strict expected count were retained. The original stopped
receipt remains available. This is not a retry of a passing-negative control.

## Scope and remaining acceptance

This component does not implement procedural watcher avoidance or queued GPU
upload/quiet foreground installation. It establishes no input/playback P95,
native crash fix, final combined-workspace or cross-platform acceptance. The
primary combined gate on the base source failed its performance assessments,
one offline metadata test and a native `pointwise_fusion` abnormal exit; those
failures remain separate. PR 575 stays a draft and issue82 stays open. Android
is retired and excluded by user direction.

## Evidence

Archive: [source fault ordering gate](data/native-prerender-source-fault-order-v2.zip).
SHA-256: `da5e03801217b0aea5accb5118fb55b6b85502f2cd8da36078a6ed6889b0d12c`.
All 31 members were read back byte for byte. The terminal receipt records the
exact original/fixed source hashes, commands, actual counts and unchanged-source
checks. The archive includes original negative failures, fixed outputs,
regression/default sink results, production patch and pre-fix production files.
