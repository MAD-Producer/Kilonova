# Current-main prerender integration (#82)

The branch was rebased onto main
`ac65d279c57626d7aa7e28061a1a7fdf7889c68a`, retaining its 84 local commits.
Backup branch `codex/82-before-main-ac65` preserves the original `a3deff79`.
No final acceptance is inferred from rebasing or old-source passes.

Integration retains main's typed Engine errors, asynchronous frame continuations,
preparation observations and nonblocking Shell `PresentPoll`. The producer's
input-generation token now travels through continuation metadata, without
recapturing newer input when an older frame resumes. `PresentPoll` uses the same
dispatch-time preemption/token capture as ordinary Present; frozen frames still
cannot acknowledge new composition input. Optional polls park only when no
completion needs acknowledgment. Grouped views retain their independent native
render path and cannot consume or produce the single-view optional cache.

The adapter-sharing constructor also forwards main's optional explicit effect
registry. The existing surface-error assertion now formats the typed error
before its unchanged substring expectation. No expected result, threshold,
deadline, ignore, stack size, GPU backend or compiler policy changed.

## Actual current-source checks

The tested identity is `ae5c1e18` plus the archived source diff SHA-256
`10f9f056f194c1df5f20ed3482afede3bcc4c7b66f81f662a17924dbe05f2a87`.
Every original command froze its recorded source through terminal collection.

- The first offline check refused before compilation because the E cache lacked
  main's locked `notify` package. Exact locked dependency fetch completed; no
  manifest, version or lockfile was changed
- The next check failed because the new constructor boundary omitted the registry
  argument; the subsequent check caught the old string assertion against typed
  Error. Both original failures remain archived with their source diffs
- After those distinct corrections, `cargo check -p motion-desktop --tests` passed
  with FFmpeg and the existing AI-depth features
- Complete unchanged Python CI discovery passed: 136 tests, 135 passes and one
  original skip. It is a tool gate, not native/GPU acceptance
- One native test build passed. Ten selected tests passed: four polling protocol
  controls, blocked-worker in-flight/queue inspection, frame shutdown/error
  identity, actual polling input-generation/frozen-frame pixels, native timer
  parking/recovery, grouped-view retirement/recovery and the existing deferred
  frame completion test. Every selected test count was checked

The deferred-frame test supports an early ordinary-presentation result when the
driver lacks pipeline-cache support. Its optional detailed report was not
retained by this controller. Therefore its successful exit is **not** evidence
that the blocked-restore callback and new resumed-input assertions executed.
That activation remains unqualified; the separate actual polling-generation
control did execute its explicit old/current/frozen assertions and pixel readback.

The [archive](data/native-prerender-main-ac65-integration-v1.zip) and
[manifest](data/native-prerender-main-ac65-integration-v1.json) retain original
controllers, receipts, logs, source diffs and relevant authored source snapshots.
Every member passed CRC and byte-for-byte readback. No native binary, third-party
runtime, original media or upstream implementation is redistributed.

Full current-source workspace regression, exact-head cross-platform CI, actual
shared-window MCP, strict whole-app input P95 and constrained-device qualification
remain pending. The original strict pair failure, full-workspace/native crashes
and unsupported platform-full-volume boundaries remain recorded. PR #575 remains
a draft; #82 and the complete 3D route remain incomplete. No panels, layout,
style or editor gesture behavior changes are introduced.
