# Native prerender source freshness (#82)

Status: the media/worker source-freshness component passed its actual native
controls and unchanged existing regression. The Engine-native source delivery
and presentation control also passed. Full regression, strict input/playback
P95 and current-head CI remain pending. Do not
use this component alone to close #82. Source parent `8fd81cf8`, based on main
`ab779cee`; the archive records every source hash. Android is retired.

The background cache worker preparation observes the project-owned visual source closure:
images, fonts, video bytes and their PTS indexes, model containers and editable
node resources. It arms native, nonrecursive parent/ancestor watches before
opening source handles or copying bytes. It validates project containment and
rechecks resolved targets after arming. Both in-project symlink routes and their
resolved targets are observed. Audio-only inputs remain outside this visual
producer, just as they do in its frozen snapshot.

An observed write, replacement, removal, directory rename or backend rescan
closes an in-memory validity gate and cancels the closure's currently attached
operation. The callback does not enqueue an Engine command or wait for its
serial worker. The native operation retains its lease until real acknowledgment;
cancellation does not release CPU/GPU admission early. Completed cache lookup,
new scheduling and completion installation check validity. A stale closure
cannot cancel a replacement closure. Frozen bytes and caller-held GPU frame
owners remain owned until their actual lifetime ends.

Limits are 4,096 source paths, 128 watched directories and a 1 MiB retained path
budget. Only one cancellation ticket and a 512-character first cause are
retained for a closure's lifetime. The cause resets with a newly prepared
closure, not on every frame. Source parent entries are filtered so sibling
logs/private snapshots do not repeatedly invalidate a healthy closure. Access
events are normal; rescan takes precedence over access/path filtering. Native
watcher faults fail closed and require the existing explicit policy recovery
boundary. Ordinary source replacement is a normal invalidation, not an Error.

The observer uses pinned, unmodified `notify` 8.2.0 under CC0-1.0. Exact authors,
revision, checksum and scope are in `ATTRIBUTION.md`. Windows path comparison
uses the OS ordinal uppercase table and normalizes extended-length prefixes;
it does not lowercase/decode paths lossily or perform IO in the callback.
Every healthy Present validity check is memory-only. Policy disabled at startup
still creates no Coordinator worker or source watcher.

Executed controls:

- Actual filesystem atomic replacement with equal byte length and restored
  modification time, with the old handle still open
- Actual in-place write, ancestor rename, watcher retirement and cancellation
  without an owner command queue
- Attachment/change race, single unacknowledged lease, stale acknowledgment,
  bounded first cause, rescan ordering and sibling-log filtering
- Actual native GPU red cache, same-path replacement without a document edit,
  refused stale lookup, rehashed green pixels and retained old red frame owner
- Actual native render blocked at Draw; source callback cancels its control
  before owner polling, retains admission until acknowledgment and rejects pixels
- Default-level real desktop logger: Info start/change in file, Error setup
  cause in file and stderr, no observation records in stdout

The OS delivers notifications asynchronously. A lookup which linearizes before
delivery and already submitted GPU work cannot be revoked. Network/remote
filesystem notification delivery is not qualified by this local-first profile;
no polling/hash scan is added to the foreground. This component does not change
the foreground renderer's independent source-reload policy, promise recovery
from a partially written/corrupt source, add retries/restarts or qualify P95.
Current full regression, unchanged input/playback P95 criteria and current-head
cross-platform CI remain required. Earlier failed P95/crash receipts remain failures with unconfirmed
native-crash causes.

## Executed proof and failure dispositions

Final controller v7 passed 8 source-watch tests, 3 actual native Coordinator GPU
tests, all 40 media prerender regressions with 1 existing manual precision
acceptance ignored, the actual default-level desktop logger test, desktop
`cargo check`, provenance and tool inventory. The four-precision manual gate was
not executed by this component controller and is not counted as accepted here.
No existing assertion, return value or test selection was changed to pass.

An added native fault regression first failed against the implementation:
0 passed, 1 failed, at the assertion that a watcher fault must not prevent
acknowledging real completed native work. The fixed fixture then passed both
actual cancellation and simultaneous controlled worker-panic cases. Source
faults keep lookup/admission closed, the actual acknowledgment releases its
CPU slot and scheduler ticket, and a worker's original failure remains primary
when a watcher error also exists. Logging occurs outside observation locks.

The original v3 compile failed at the sibling-module visibility boundary; only
that visibility was corrected. v4 reached 7 passed/1 failed because its Windows
MoveFileEx fixture could not replace the open destination. Independent owned
Windows API controls confirmed that ReplaceFileW succeeds with that retained
reader; the cfg(test) fixture now uses that documented operation and additionally
checks that the old reader still sees old bytes. Equal size/mtime and native
cancellation assertions remain intact. Reference:
[ReplaceFileW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew).

v5 reached 38 passed/1 failed/1 ignored in existing media regressions: applying
live invalidation to the standalone frozen-snapshot API changed its established
lifetime contract. Live observation is now explicitly enabled only by cache
worker preparation. Standalone `PrerenderPlan::prepare` remains detached from
live project source lifetime, and the unchanged deletion-after-freeze pixel
test passes. v1 failed fetching into the existing read-only C registry; v2's
lock verifier rejected notify's expected mio/log feature edge. Subsequent
validation uses an E Cargo home; all existing versions/checksums and edges
remain pinned except that exact added feature edge. Original failed receipts
are retained; none represents a hidden successful or skipped execution.

The previous input study still aborted in Rust BTreeMap/serde_json serialization
with `0xc0000409`; its incomplete samples remain rejected. The earlier full gate
still failed with `0xc0000005` in magnified_effect_density. One diagnostic run of
the exact preserved CPU-test binary under Windows debugging completed without
reproducing that crash. It does not repair the full gate or establish a cause.
Strict input and playback P95 failures also remain failures.

Archive: `data/native-prerender-source-freshness-v1.zip`.
SHA-256: `d661eda86ee2f513264101c05ec58f052e53999160497eb49e6ba14d66bc8cb5`.
All 80 members were read back byte for byte. No executable, PDB, dump,
user project, source payload or external product asset is included.

## Engine-native delivery and presentation proof

On source parent `04c0f5ea`, the actual Engine test passed 1/0/0; desktop check,
provenance and inventory also passed. A native High preview first showed red
image pixels, then consumed an actual cached red frame. With Engine blocked in
TestWait, the test wrote the same source path without a document edit. The
published observation became invalid while in_flight remained test_wait and
queue_depth stayed zero: source delivery did not need the blocked command queue.

Seek and Present were queued before releasing TestWait, so the next presentation
could not be satisfied by a newly warmed generation racing ahead of that queue.
The old cache was refused: cachedPresented stayed 1. Subsequent actual background
warmup and native presentation showed green pixels and increased cachedPresented
to 2, while the authored revision stayed 0. This is the application consumer's
source gate and retained-front pixel proof, beyond the media-only controls.
The status metadata now names native_visual_source_watch.

The first foreground fallback after invalidation keeps its existing independent
image reload behavior; the test does not promise to reload that cache or change
that product policy. It verifies that stale optional cached pixels are refused
and newly frozen source pixels reach subsequent cached presentation. The
native loader still reports the existing missing Epic overlay JSON paths; this
passed control neither suppresses those errors nor assigns crash causality.
No UI layout, style, panel or interaction contract was modified.

Engine archive: `data/native-prerender-engine-source-v1.zip`.
SHA-256: `e8fbfdb51537907446187201686b29449003ad711754efe199c97d90f19e3ba1`.
All 14 members were read back byte for byte. The archive contains only
owned fixture logs, raw pixel/state evidence, source hashes and controller.
