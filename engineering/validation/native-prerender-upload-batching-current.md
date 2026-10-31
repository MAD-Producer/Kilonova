# Native upload batching candidate

Issue: [82](https://github.com/MAD-Producer/Kilonova/issues/82).
Base source: `510484160fde38d18b177ba8c69baf19af1730dc`.
Candidate disposition: playback checkpoint passed; native failure unresolved;
integrated-source qualification still required.

## Behavior

The coordinator's healthy foreground native-cache installation previously
emitted Info and flushed the sink for every completion. It now keeps that
Present path quiet; installation timing is Debug only when at least 100ms.
Background worker Info start and its pre-operation flush remain available.
Engine in-flight publication is unchanged. This removes synchronous log IO
from the healthy installation path without changing cancellation or replies.

The coordinator uses an explicit install-for-next-submission API. Native bytes
are copied into queue staging; destination admission is retained before queue
IO, through cache eviction and actual consuming submission retirement. Renderer
shutdown flushes abandoned queued writes using the existing upload owner. The
immediate installation API retains its original submission boundary. This is
not zero-copy, a new output conversion, format downgrade or ownership transfer.

## Genuine negative controls and actual results

Three new controls failed on the original behavior for the intended assertions:
healthy foreground installation flushed the logger; upload submitted a separate
empty command; and eviction released an unsubmitted destination's admission.
The isolated logging negative produces child and parent failure records for one
control. All existing assertions and test bytes remained unchanged when applying
the production fix.

The fixed foreground logger control passed and checks that background worker
start remains visible at the default Info threshold. The forced four-precision
pixel control passed and checks exactly one upload/copy submission, storage bytes,
HDR/negative/alpha preservation and destination lifetime during consuming copy.
It also clears cache ownership before submission.

The subsequent normal lifetime control ABORTED with `0xc0000409`, without a test
summary. Its stack enters builtin WGSL initialization before queued upload. The
original EXE/PDB/dep-info and stderr were preserved and hashed before recompiling.
The panic reports thread 79108; that is not a process ID. One execution of that
exact preserved binary under a debugger and standard heap passed the one test;
there was no second-chance exception or dump. This does not establish a cause,
fix or normal-gate acceptance. The original failure remains failed. Neither
that unchanged normal test nor its enclosing cache group was rerun to obtain green.

A separate continuation collected only previously unexecuted independent gates:

- Invalid/foreign/stale queued completion refusal: one passed
- Existing forced immediate four-precision storage control: one passed
- Media prerender group: 41 passed, zero failed, one existing manual precision
  control ignored under its original selection
- Actual desktop default blocked-worker Info start/file, Error stderr and stdout
  isolation: one passed
- Actual native Engine warm presentation/edit invalidation: one passed
- Changed-source paced playback: one passed, all 240 frames completed
- Desktop check, provenance and inventory: passed

The isolated capture logger and actual desktop sink controls establish separate
boundaries. Groups overlap and counts must not be added. The prior v2 controller
rejected zero selected tests from a stale cross-worktree binary. v3 refreshed
only package-root mtimes, verified source bytes unchanged and proved the actual
test inventories before executing genuine negatives. That failure is retained.

## Strict local playback assessment

The unchanged independent parser accepted all four 60-frame episodes and both
original nonregression pairings: ratios `0.7461560230166966` and
`0.9446954140999315`. Pooled ratio: `0.755955955955956`. Enabled episodes had
60 actual cache hits each and zero missed next deadlines. Pairing, thresholds,
samples and existing assertions were not relaxed.

These measurements cover this exact candidate only, not the later combined
watcher changes, whole-app input P95, universal hardware, physical low-memory
qualification, a native crash fix or final workspace/cross-platform acceptance.
Optional disk-cache/quota/full/corruption requirements remain pending. Android
is retired and excluded. PR575 remains a draft and issue82 remains open.

## Evidence

Archive: [upload candidate evidence](data/native-prerender-upload-candidate-v4.zip).
SHA-256: `18448e5af84510e9f6f74f11ad4d8f53fc4505c2175c27a0f680e218924f79a7`.
All 58 members were read back byte for byte. Includes failed and completed
terminal receipts, exact source hashes, original/fixed logs, applied patch,
unchanged strict assessment and raw 240-frame sample data, bounded native failure
disposition and prior zero-test rejection. Private binaries and raw process
reports are excluded from the public archive.
