# Tracked plate replacement through MCP

This increment exposes the paired video/solve transaction from #583 through the
existing editor's MCP router and stdio server. It does not add panel controls,
implicitly run a tracker, bake camera keys, or transcode a source. Issue #88 and
the separate real-footage quality gate for the optional planar prototype remain
open. Android is not maintained or included in acceptance.

## Workflow

Read `motion_state` first. The project must already contain a video with attached
tracking originals. Use `tracked_video_replacement` with one of five actions:

1. `prepare`: pass a unique `requestId`, a video `target`, local video `path`,
   `expectedRevision`, `expectedDocumentGeneration`, and explicit `withAudio`.
   Optional `track` and `audioTrack` choose container tracks. Preparation checks
   context and editor gestures on the owning worker, then starts the existing
   asynchronous media job. It changes no authored state or undo history.
2. `status`: poll with `requestId`. Wait for `state: "ready"`; `failed` and
   `cancelled` are terminal outcomes, not permission to call finish. The result
   includes the existing task phase, progress and original error information.
3. `finish`: supply the prepared request ID, target, current revision and document
   generation, `solves` (`asset` ID plus local original `path`), and explicit
   `bindings`. Every attached solve must appear exactly once. An empty bindings
   array preserves bindings; a binding update with explicit `binding: null`
   detaches that target. Point, plane and camera payload schemas come from the
   core catalogue. The existing host verifies the original bytes, decoded
   dimensions, exact native PTS and whole-project bindings before one saved
   transaction. The result's `state` is the committed editor snapshot.
4. `cancel`: cancel unfinished preparation without editing the project.
5. `release`: release a committed, cancelled or failed task. Unfinished tasks must
   be cancelled first. Task cleanup uses the existing media lifecycle.

For example, after reading revision and document generation:

```json
{
  "action": "prepare",
  "requestId": "replacement-1",
  "target": {"kind": "video", "id": 1},
  "path": "E:/Media/new-plate.mp4",
  "expectedRevision": 5,
  "expectedDocumentGeneration": 1,
  "withAudio": true
}
```

After the task becomes ready, read state again and finish explicitly:

```json
{
  "action": "finish",
  "requestId": "replacement-1",
  "target": {"kind": "video", "id": 1},
  "expectedRevision": 5,
  "expectedDocumentGeneration": 1,
  "solves": [{"asset": 2, "path": "E:/Media/new-solve.mstrack"}],
  "bindings": []
}
```

Read-only agents may query status; prepare, finish, cancel and release require
editing access. The advertised tool's conservative `readOnlyHint` is false.
Every action rejects unknown fields, including fields belonging to another
action. Requests are bounded to 256 KiB, task IDs to 128 bytes, paths to 4096
bytes, solves to 256, and binding updates to 1024. Duplicate or zero solve IDs,
non-video targets and missing explicit nullable binding fields are rejected.
The existing media registry additionally bounds retained tasks and workers.

An edit or project replacement may supersede the caller's context. Re-read
state and decide whether to cancel or prepare again; the tool does not retry or
silently adopt a newer revision. A blocking-call timeout likewise does not prove
that a queued commit failed. Observe current project and task state first.

## Evidence boundary

The [receipt](data/tracking88-mcp-replacement-v1.json) and
[source/log archive](data/tracking88-mcp-replacement-v1.zip) retain the commands,
source and binary hashes, real JSON-RPC output, default-level logs, stderr and
earlier unsuccessful attempts. All owned validation processes are terminal.

- Three focused tests passed, including native asynchronous video preparation,
  source mismatch rejection, stable-ID commit, saved project bytes, managed solve
  originals, undo/redo and cancellation. The native test completed in 4.76s.
- All **12 MCP tests passed**, with 0 failures and 0 ignored, in 13.15s. This
  includes the existing permissions, history, malformed-input and tracking-import
  logging tests; the twelve are not all newly introduced tests.
- `cargo build -p motion-desktop --bin motion-studio --features ffmpeg --locked
  --offline` succeeded. The resulting executable was then launched with actual
  stdin/stdout pipes, `--mcp` and a copied synthetic video project.
- Ten actual stdio controls passed: tool discovery, stale/unknown input rejection,
  cancel/release, preparation without project publication, wrong-original and
  document-generation rejection, one-revision commit, atomic history and save,
  default-level starts and Error file/stderr output with pure JSON-RPC stdout,
  separate-process reopen, and read-only mutation rejection. The router also
  checks read-only status on a ready task; the separate read-only process has no
  retained task and correctly dispatches status to a missing-task response.

The first attempt failed compilation because the new native test omitted its
`Platform` trait import; only that import was fixed. A second attempt printed
three passing tests, but its original exec handle subsequently disappeared, no
matching validator process remained, and the receipt contained only NUL bytes.
Its build log also ended in NUL bytes. Those files are retained without claiming
a complete successful build. The third run revalidated unchanged Rust source,
completed the build and produced a valid receipt before stdio acceptance.
The cause of the incomplete second record is not established.

This is a focused acceptance and MCP regression, not a new full desktop or
workspace regression. The three native subprocess crashes from #583 remain
unresolved. It also does not exercise audio-device output, add new panel
interactions, or establish the optional tracker's real-footage quality. No
panels, UI style, dependency or third-party asset change is part of this increment.
