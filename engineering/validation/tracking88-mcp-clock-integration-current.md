# #88 MCP workflow on the integrated layer-time baseline

PR #584 integrates parent #583 `6e2ea59a1380654f7f6cb769f0a4c7aa3b984cf6` without textual
conflicts. Main's layer-time behavior and paired video/solve replacement both
remain present. No new product behavior, UI, dependency or asset is authored.
The entire staged model, core, host, media and render crate trees equal that
parent byte for byte; their Git tree IDs are recorded in the receipt. The parent's
1060-pass/one-existing-ignore result is historical evidence for that parent and
its unchanged crates, not a newly executed full desktop regression for this PR.

## Current-source native verification

- Three existing MCP replacement tests passed, including actual asynchronous
  preparation, wrong-original refusal, cancellation, atomic publication/history
  and the fixture used by the independent stdio test
- One actual Windows Surface test passed, zero failed/ignored, exercising the
  native video decoder, successful Present, retained-front pixel identity,
  invalidation/reuse counters, refusal, replacement, undo/redo and fresh reopen
- The FFmpeg-enabled desktop executable built successfully; its SHA-256 is
  `10e684927c5559979f5a5c3ddb5c31256f3977672216ec1643521f37071b0ea3`
- That exact executable passed all ten existing stdio workflow controls via real
  pipes and a separately opened read-only process

The wire controls verify five advertised actions; stale/unknown-field rejection;
cancel/release without author edits; native ready preparation without publication;
wrong-original/document-generation rejection; one-revision stable-ID commit;
atomic undo/redo/save; default Info starts, original Error in both file and stderr
with every stdout line valid JSON-RPC; fresh-process saved-project reopening;
and read-only mutation denial with status dispatch still available. MOTION_LOG
is unset. Existing assertions and protocol/product behavior remain unchanged.

The Surface uses a real hidden native window, its owner-thread event loop and
the committed scratch front. It does not prove physical display scanout or all
codec, lens, point/plane or mapped-clock combinations. The separate native
footage-clock and paired/audio controls remain in the parent's evidence.

All Rust, shader and Cargo source hashes stayed frozen across the commands and
stdio process. Six relevant desktop source snapshots match staged LF-normalized
Git blobs. The [receipt](data/tracking88-mcp-clock-integration-v1.json) and
[member-by-member verified archive](data/tracking88-mcp-clock-integration-v1.zip)
retain command results, current-source hashes, actual process exits, native
images/counters, JSON-RPC streams and default logs. No panel differs from main.

## Remaining boundaries

This is a focused current-source acceptance. The previous 1542-pass full desktop
package belongs to `52ce7ebf`; it is not repeated or relabeled as this source's
full regression. Earlier native access violations remain unexplained. Final-head
cross-platform CI, full maintained regression, photographed planar-tracker
quality and full #88/3D-route acceptance remain separate work. Android is retired
and excluded. No GitHub PR merge or issue closure is implied by these results.
