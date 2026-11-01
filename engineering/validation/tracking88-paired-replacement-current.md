# Atomic tracked-video replacement increment

Issue [#88](https://github.com/MAD-Producer/Kilonova/issues/88) remains open.
This increment depends on PR #581 and is submitted separately from the optional
planar tracker in PR #582. It adds an explicit host transaction for a prepared
new video, every original solve attached to that video, and requested binding
changes. It is not a completed tracker UI or the whole issue's acceptance.

## Behavior

Ordinary command batches validate after each command. Replacing only the video
can therefore fail against the old solve before a later solve replacement runs.
`ReplaceTrackedVideo` instead builds and validates the complete candidate, then
publishes one authoring revision and one undo step. Video and attachment IDs
remain stable; immutable old video and original-solve files remain available for
undo. Unmentioned authored animation and corrections remain unchanged.

The host `finish_tracked_video_replacement` media request consumes an existing
ready video task. It requires the current revision/document generation, every
attached solve exactly once, and explicit composition/object addresses for any
point, plane or camera binding changes. Null explicitly detaches a binding;
missing nullable fields are malformed rather than silently defaulted. Duplicate
addresses, foreign sources, incomplete solve sets and reused mutable paths are
rejected. The complete candidate must still validate after all changes.

Original-solve reads verify the new task's encoded source SHA, native decoded
dimensions and exact original PTS before entering the publication session guard.
The temporary verification view does not change author state. Existing media
publication owns video/audio/cache publication and exact prepared-task identity.
The paired edit preserves original raw document bytes. A reported post-publication
durability failure keeps model/history and referenced files aligned and returns
the acknowledgement error after the media publisher retains its resources.
This latter failure path still needs dedicated injection in this increment.

The request accepts at most 256 solves, 1024 binding updates and 16 MiB aggregate
original-document metadata size. Info admission/verification starts precede cold
source work. Failures retain bounded request/source context and log Error outside
session guards. Superseded requests remain normal Debug events.

## Actual verification

The archived receipts, logs, changed source snapshots and hashes are in
[the evidence manifest](data/tracking88-paired-replacement-v1.json) and
[the evidence archive](data/tracking88-paired-replacement-v1.zip). Raw SHA values
identify the validated working files; LF hashes identify their Git blobs. The
controllers checked that source did not change while their validator was live.

- `cargo test -p motion-core --test tracking_replacement --locked --offline -- --test-threads=1`:
  **4 passed, 0 failed, 0 ignored**. Controls cover atomic camera rebinding,
  authored-track preservation, one-step undo/redo, incomplete sets, invalid
  rebinding with preserved redo, duplicate IDs/addresses, immutable-path reuse,
  foreign sources, active gestures and required nullable JSON fields
- `cargo test -p motion-desktop --bin motion-studio --features ffmpeg --locked --offline -- engine::tracking_video_replacement_tests::paired::fresh_plate_and_solve_are_atomic_native_verified_undoable_and_reopenable --exact --test-threads=1 --nocapture`:
  **1 passed, 0 failed, 0 ignored**. Uses existing credited video fixtures and
  actual FFmpeg/GPU rendering. Rejects old-source SHA, shifted native PTS and
  stale revision; publishes the correct new pair; checks changed rendered
  pixels, stable IDs, original byte preservation, GPU undo/redo, and reopening
  after the external solve file is deleted
- With `MOTION_LOG` unset, the native run observes the new Info start and original
  verification phase in the file and the actual source mismatch in both file
  and stderr. The test harness stdout is not an MCP protocol-isolation proof

The native log also contains this machine's existing missing Epic Vulkan overlay
manifest errors and ordinary asynchronous capture-pending retries. They did not
prevent the asserted completed renders. They are retained in the raw evidence.
No existing assertion was weakened or changed. No dependency, third-party asset,
panel, layout, interaction or Android code was added by this increment.

## Pending acceptance

- Dedicated rejection after original staging and publication rollback controls
- Paired save failure, published durability acknowledgement and unwind injection
- Point/plane bindings, explicit detach and nested-composition controls
- Full affected-crate/desktop regression and cross-platform CI for the final head
- Product-facing tracker preparation/finish workflow and protocol output proof
- Exclusion or detection of external file writes between verification/publication
- Remaining #88 requirements, including the independently failing photographed
  planar-tracker quality acceptance documented on PR #582

These are explicit draft limitations. The targeted successful runs do not establish
complete fault safety, full regression, UI acceptance or issue completion. Android
is stopped and excluded from the maintained desktop delivery scope.
