# Managed original tracking import

Follow-up: [original decoded source admission](tracking88-decoded-source-current.md)
extends this checkpoint with actual image decoding and exact native video PTS
verification. The receipts and response fields below describe this earlier
encoded-source-only checkpoint.

Issue #88 remains open. This increment connects canonical tracking JSON originals
to the desktop serial worker and the headless MCP boundary. It does not apply a
camera, plane or point solve to a layer. No panel, layout, visual style, menu,
dependency or Android code changes are included. The fetched main baseline is
`8573d9975cf70d2c8fe05605102ec9d744816070`; the preceding attachment commit is
`7272e661fafcf84a763776f5556e4e0c626f139c`.

## Implemented boundary

`import_tracking` requires a bounded local `path`, an explicit image/video
`source` reference, `expectedRevision` and `expectedDocumentGeneration` from the
current state. Unknown fields, malformed source references, read-only clients,
active gestures and stale document contexts are rejected before an authoring
change. The tool schema describes its limited attachment scope. Successful
responses explicitly report `decodedTimelineVerified: false` and `poseApplied:
false`; encoded-byte verification must not be confused with a solved scene.

The worker publishes its existing in-flight state and an Info start under the
`import_tracking` diagnostic name before entering the handler. The host operation
also logs its request, session, selected source and read phase before file IO.
Source reading, strict decoding, source hashing and owned original staging happen
outside the session guard. The owner thread rechecks the document generation,
root, complete project and revision before committing one saved undoable edit.
IO and rollback errors retain original causes; the host reports failures after
releasing the session guard, with bounded context and elapsed time. Existing
Present and reply-discard behavior is unchanged.

Expected cancellation and superseded import requests keep their rejection on the
wire and are observed at Debug, without passing through the generic Error logger.
The existing generic `respond` function and its best-effort reply discard are
unchanged. Cleanup failures still qualify as faults even when their primary cause
was supersession.

The original document is bounded to 16 MiB, including growth during reading. The
selected managed source is canonicalized within the project and freshly hashed.
File metadata is checked around acquisition. The importer allocates in the shared
image/audio/video/model/tracking resource namespace, preserves the complete
original bytes, exclusively creates its managed file and syncs it before saving
the project. Unix directory entries are also synced before project publication.
Tracking directories must be direct managed directories; source paths must remain
inside the canonical project root. Verified preexisting originals are reused
without claiming ownership; conflicting originals are never overwritten.

The stage owner lives outside the session guard. Rejection or a known failure
before publication rolls back author state/history and removes only its newly
created original. Cleanup failures are secondary causes alongside the primary
failure. Undo retains managed bytes for redo even after the external original is
removed. An acknowledged publication commits matching author state/history. If
project bytes were published but their durability acknowledgement fails, matching
memory/history and the managed original are retained, while the original error
still returns to the caller. This local transaction does not change the existing
generic core save API.

## Verification

Final native acceptance completed at `2026-11-01T05:11:44.308827+00:00`.
One owned Cargo/GPU/native controller ran at a time. Unchanged failed
full/performance gates were not rerun as evidence for this increment.

| Actual control | Exit | Result |
| --- | --- | --- |
| Import V1 desktop check | 0 | typed worker/host routing compiled |
| Import V1 tracking test compilation | 101 | ambiguous new test `Error` import; retained before correction |
| Import V2 tracking90 | 101 | 89 passed / 1 failed / 0 ignored; missing original IO cause context |
| Import V3 desktop check | 0 | compiled |
| Import V3 tracking90 | 0 | 90 passed / 0 failed / 0 ignored |
| Import V3 new MCP tests | 0 | 3 passed, plus five zero-selected integration targets |
| Native V3 full host unit gate | 0 | 203 passed / 0 failed / 0 ignored |
| Native V3 all MCP unit tests | 0 | 9 passed / 0 failed / 0 ignored |
| Native V3 build and actual MCP processes | 0 | import/save/reopen passed; supersession Error misclassification retained as a negative control |
| Native V4 desktop check and build | 0 | final desktop correction compiled |
| Native V4 all MCP unit tests | 0 | 9 passed / 0 failed / 0 ignored |
| Native V4 bootstrap/import/reopen-readonly | 0 | three real application processes; default file/Error stderr/protocol isolation, persistent bytes and history verified |

V1 was a new-test import-name error. V2 exposed missing save phase/path and cause
context; production error wrapping was corrected while its assertion stayed
unchanged. The V3 unit controller rejected additional zero-selected Cargo targets
after the actual 3/0/0 result. Its failed controller receipt and exact counts remain
unchanged. The subsequent controller reconciles the raw output and broadens the
explicit desktop binary target to the six existing MCP tests as well.

Native V3 recorded the expected stale request incorrectly passing through the
generic Error logger. V4 changes only the desktop import reply branch and its new
regression assertions. All host code and dependencies are byte-identical to the
203/0/0 gate; the full host gate is linked rather than repeated. V4 verifies that
the rejection remains on the wire and is absent from default Error output.

The [public evidence archive](data/tracking88-managed-import-v4.zip) contains 104
members, including exact changed-source snapshots verified against their original
source manifests, failed and successful receipts, raw Cargo/native stderr logs,
recorded native protocol replies and independently generated fixture resources.
SHA-256: `9e5ba562e7f4db7098501982a7fa172b9327c3365e3e170acbdba028e3f27f8e`.
No executable, PDB, private Windows events or user project content is published.
Native reply transcripts are parsed/re-serialized JSON records; the controller
also verifies every stdout line and rejects trailing non-protocol output.

Static closeout also passed staged whitespace, existing panel style consumers,
provenance (5,129 tracked paths) and validation-tool inventory (57 tools; zero
Android selectors/tests). No panel or dependency path is staged. These separate
local receipts are retained in `artifacts/pr-queue/tracking88-import-closeout-v4`;
they are not additional members of the public archive above.

The new controls exercise actual generated PNG bytes, original JSON formatting,
save/reopen, undo/redo, stale context and active gestures, a real temporary-file
creation failure, verified and corrupt preexisting originals, tampered encoded
source bytes, cancellation and rollback IO failure. The acknowledgement and panic
controls perform a real project publication before injecting their failure.
Original assertions are retained when a failure exposes missing cause context.

## Remaining limits

Only the independently authored canonical JSON format is imported here; external
vendor-format adapters remain separate work. Image/video dimensions and video
sample counts are compared with project metadata, not independently decoded
original frames. Source PTS agreement, proxy correspondence, calibrated real
footage and relink/cache invalidation remain unverified. File verification is a
description of acquired bytes, not a lease against arbitrary concurrent external
filesystem mutation. Cooperative cancellation cannot interrupt a blocked OS read.

If unwinding occurs during a save with uncertain publication, the original is
conservatively retained. The core restores author memory during unwind and the
existing worker panic path closes the session; the published disk project can
still reopen. A crash before publication can leave an unreferenced owned file;
this increment does not add automatic orphan deletion or recovery retries.

Independent camera/object/parent pose bindings, source-time evaluation, rendering,
runtime cache invalidation and maintained-desktop cross-platform CI remain required
for #88. Android is out of the maintained acceptance scope by user instruction.

The earlier native `0xc0000005` in the preview-pressure child remains unexplained;
its original binary, symbols and private Windows events remain retained. Passing
tests on this changed source must not be described as a fix for that failure.
Issue #82 also retains its original failed strict P95 performance results.
