# Procedural source watcher ownership

Issue: [82](https://github.com/MAD-Producer/Kilonova/issues/82).
Base source: `510484160fde38d18b177ba8c69baf19af1730dc`.
Component gate completed: `2026-10-30T17:19:58.200960+00:00`.
Disposition: accepted procedural source watcher component only.

## Change and boundaries

Procedural-only frozen closures have no filesystem entries to watch. They now
retain the same validity observation, cancellation lease and retirement boundary
without allocating a native watcher or its event thread. Root validation and
nonempty source identity validation remain unchanged. Visual closures still
create and arm the real native backend and preserve its original failure cause.
The backend factory is private; production continues to use notify's existing
recommended watcher. No dependency, retry, timeout, UI or output change is added.

## Actual verification

The new empty-source control first failed against the old implementation for
the intended assertion: procedural sources unnecessarily required the backend.
The nonempty-source control passed before and after the fix and retained the
original backend error. Both fixed controls passed without changing test bytes.
They also exercise real observation/lease cancellation and retirement ownership.

- Source watcher group: 10 passed, zero failed
- Media prerender group: 42 passed, zero failed, one existing manual precision
  control ignored under its original selection
- Existing desktop default source-watch file, Error stderr and stdout isolation
  control: one passed
- Provenance and tool inventory: passed

These groups overlap; their counts must not be added. The injected backend
failure establishes the selection boundary, not general native OS reliability.
The actual desktop sink control is separate from the injected failure fixture.
All existing assertions and all test bytes were retained between negative and
fixed execution. Only source_watch.rs production bytes changed at fix application.

The preceding v2 controller rejected a cached test binary: exact-name selection
executed zero tests. That receipt remains failed and is included. v3 refreshes
only package-root modification times, verifies source hashes unchanged, and
requires the actual built test inventory to contain both new tests before any
negative control. It then obtains the real negative failure and fixed passes.
This changes build freshness, not the product, test assertions or expected counts.

## Remaining qualification

This component does not prove upload batching, foreground log suppression, input
or playback P95, native crash resolution, final integrated workspace regression
or cross-platform acceptance. The independent full gate on the base source failed;
its original failures remain. Android is retired and excluded by user direction.
Issue82 remains open and PR575 remains a draft. Combined fault-order/procedural
source behavior requires verification after integration on the current baseline.

## Evidence

Archive: [procedural source watcher controls](data/native-prerender-procedural-watch-v3.zip).
SHA-256: `639a6516323264ee98376423d741626e5effd5a81b1d9e2fb8436c3848ee498a`.
All 30 members were read back byte for byte. The archive includes the prior
zero-test rejection, genuine negative failure, unchanged fixed controls, regression
and default sink outputs, controller, original production file, applied patch,
exact source hashes and terminal receipts.
