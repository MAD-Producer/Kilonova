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
Dedicated save-policy and actual publisher injections are recorded below;
their controlled-probe scope is distinct from the native media controls.

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

- Native-video/audio publisher and desktop worker recovery under post-publication faults
- Full affected-crate/desktop regression and cross-platform CI for the final head
- Product-facing tracker preparation/finish workflow and protocol output proof
- Exclusion or detection of external file writes between verification/publication
- Remaining #88 requirements, including the independently failing photographed
  planar-tracker quality acceptance documented on PR #582

These are explicit draft limitations. The targeted successful runs do not establish
complete fault safety, full regression, UI acceptance or issue completion. Android
is stopped and excluded from the maintained desktop delivery scope.

## Follow-up: real staged-publication rollback

The [rollback receipt](data/tracking88-pair-rollback-v1.json) and
[source/log archive](data/tracking88-pair-rollback-v1.zip) extend the same native
test with two actual failures after original staging and video/cache publication:
an invalid new camera track and a directory blocking `project.json.tmp` creation.
Both reject with a failed task while preserving revision, author state, reopened
saved state, and every published file byte under assets/cache plus project.json.
The test then completes a valid replacement and its GPU undo/redo/reopen checks.
The targeted FFmpeg test passed: **1 passed, 0 failed, 0 ignored, 14.01 seconds**.
Default-level Info starts and each failure's Error in file and stderr were checked.

The actual save failure exposed missing phase context in generic core IO errors.
The paired sink now wraps that original error as `save_tracked_video_project`,
including project.json and pre-publication status. Return/failure/rollback semantics
remain unchanged. The first local attempt failed because its snapshot read a
session-owned lock file. The fixture now snapshots published resources explicitly
and saves its authored baseline before comparing reopen; no existing assertion was
weakened and no production success was mocked.

Initial PR head `07cefe16` passed Rust/GPU, Ubuntu and Windows CI. Its macOS Intel
job failed the existing asynchronous movie-freeze panic test while waiting five
seconds for a reply (`failure/tests.rs:378`); raw log was inspected locally in
`artifacts/pr-queue/tracking88-pair-macos-first.log`. This is a failed gate, not a
cross-platform pass. This increment does not modify or disable that assertion.
Final-head CI remains required, together with the other pending items above.

## Follow-up: binding kinds and nested authoring state

The [binding receipt](data/tracking88-pair-bindings-v1.json) and
[test-source/log archive](data/tracking88-pair-bindings-v1.zip) cover three new
CPU controls plus the existing four core transaction tests: **7 passed, 0 failed,
0 ignored**. A root composition references an actual child composition; both
contain point and plane bindings sharing the replaced solve, with independent
authored offsets. The child remains active across the joint transaction.

Controls verify renamed point/plane track IDs in both compositions, unchanged
corner-pin effects/authored transforms/manual observations, one revision and
one undo/redo step. An independent numerical check distinguishes 20 composition
pixels of point motion from [10,5] layer-local pixels of corner-pin motion.
Explicit null point, plane and camera bindings traverse the JSON command parser
and/or transaction and preserve authored corrections. Locked child targets and
missing compositions reject the whole edit without history or partial bindings.

Early fixture attempts are recorded in the receipt: the Track API and composition
ID prefix were corrected; an added plane numerical oracle was corrected to use
the API's layer-local axes, rather than composition axes. Existing four test
assertions were unchanged. These CPU tests do not establish GPU/UI behavior or
post-publication acknowledgement/unwind safety. No production behavior changed.

## Follow-up: published acknowledgement and unwind policy

The [publication receipt](data/tracking88-pair-publication-v1.json) and
[source/log archive](data/tracking88-pair-publication-v1.zip) record **2 passed,
0 failed, 0 ignored** save-policy controls and the subsequent complete host
library unit suite: **166 passed, 0 failed, 0 ignored, 161.26 seconds**. Both runs
kept crate source frozen and checked its hashes after the validator exited.

The paired sink's persistence call is factored into a private callback; production
continues to pass the same `storage::save`. Tests write and reopen real project
bytes, retain actual `Prepared` owned originals, and inject faults only after the
save succeeds. A published acknowledgement error is retained separately while
the author model/history commits to match disk; cleanup preserves both original
documents and undo/redo remains complete. A panic really unwinds through the core
transaction, restoring memory/history while retaining all originals referenced by
the already-published document. That document remains loadable from disk.

These are **save/history/original-ownership policy tests**. Originals use actual
PNG source decoding; video metadata envelopes exercise the model publication
boundary, not a native video decoder. They neither run the enclosing VideoJobs
cache publisher nor exercise worker session closure after the injected panic.
Native FFmpeg/GPU admission/replacement is established separately by the earlier
native evidence. The combined end-to-end fault boundary, final desktop regression
and final-head cross-platform CI remain pending. No existing assertion changed.

## Follow-up: actual media publisher under post-save faults

The [publisher receipt](data/tracking88-pair-publisher-v1.json) and
[source/log archive](data/tracking88-pair-publisher-v1.zip) record **4 passed,
0 failed, 0 ignored**: the previous two policy tests and two new tests crossing
the actual `VideoJobs` asynchronous copy, ready-candidate identity, source rename,
cache-pair publication and paired save boundary. Tests verify real source bytes,
time-index existence, decoded thumbnail pixels and original-solve resource reads
before the save and again after the fault and publisher teardown.

A published acknowledgement error lets the publisher retain its successful
resources while preserving the original acknowledgement cause in the paired sink.
Disk/model/history match and undo/redo remains complete. A real panic after save
unwinds through both the paired transaction and publisher; the author model is
restored, while the newly saved document and all its source/cache/original files
remain reopenable after both publisher and original preparation are dropped.

The provider is deliberately controlled: it decodes independently generated PNG
bytes and supplies one-frame video metadata. It does not verify native video
decoding, audio publication, desktop worker shutdown or default logger behavior.
Those are separate boundaries. The initial fixture incorrectly selected the
image decoder by the neutral staging filename `source.mp4`; it now decodes the
actual input bytes by content. No production code or existing assertion changed.
The previous 166-test full-host result belongs to the earlier source; this follow-up
adds two tests and establishes the four focused controls, not a new full-suite pass.

## Follow-up: native linked-audio admission and rollback

The [audio receipt](data/tracking88-pair-audio-v1.json) and
[source/log archive](data/tracking88-pair-audio-v1.zip) retain the focused runs,
full regression, unchanged-source isolated reruns and Windows event evidence.

The new native audio control first reproduced a real admission defect: the
temporary verification view combined the new video byte count with the old
linked-audio metadata. A valid different container therefore failed with
`video/audio source timeline mismatch` before original-solve verification.
The view now applies a coherent prepared video/audio pair using the normal
media replacement action. Existing audio IDs remain stable; newly added audio
uses the publisher's all-resource ID policy. These changes affect only the
temporary verification clone; full author state is still committed once.
The ready-task token also snapshots and compares its linked-audio metadata.

Focused validation passed **9 tests, 0 failed, 0 ignored**: three media candidate
controls, five paired host controls, and one native desktop test (**12.74 seconds**).
The new host control covers audio addition, replacement and removal, allocation
beyond a removed solve's ID, and unchanged author state. The candidate control
rejects a changed audio presence or byte count and accepts a restored exact match.

The native test uses the existing credited synthetic `sound-24fps.mp4` and
`audio-delayed.mp4`, real FFmpeg picture decoding, the platform's normal portable
audio decoder, actual asynchronous import/publication, and GPU capture. Original
PTS are parsed exactly from the fixture manifest's decimal microsecond strings,
not inferred from frame rate. Invalid camera rebinding and a real filesystem save
failure must roll back every published source/PCM/waveform/index/thumbnail/solve
and preserve revision, disk project, rendered pixels and the old audible pulse.
A valid paired replacement preserves video/audio/solve IDs in one revision. Its
PCM bytes match a separate decode of the new external source, its waveform cache
exists, and independently authored pulse times distinguish the old audible head
from the new silent head and delayed audible pulse. Undo, redo and reopening after
deleting the external solve document preserve the matching project/audio/picture.
Default-level starts and actual failure records in file and stderr are checked.

Earlier attempts remain recorded: an authored fixture's placeholder video was
removed from the new empty import project; one rustc process exited with a native
access violation; the next executed test exposed the real admission defect above.
No existing assertion was changed or disabled. These controls do not inject
post-publication faults through native codecs, test desktop worker closure on
that fault, or exercise an audio output device or a new MCP endpoint.

The complete host library suite passed **169 tests, 0 failed, 0 ignored** in
149.19 seconds. The complete FFmpeg-enabled desktop **binary** suite did not
pass: **1522 passed, 3 failed, 50 existing ignored**, in 2166.92 seconds. Its
failures were the existing mask-admission, text-style export and project-precision
native-window subprocess wrappers. No test was skipped by the invocation, and no
new ignore annotation was added. This is not a full-package integration-suite or
manual native-window acceptance claim; the receipt lists the ignored tests.

Windows Application Event 1000 recorded three `0xc0000005` native crashes of the
same test executable during that full run, with faulting module `unknown`.
These records coincide with the three failed subprocess wrappers; the archived
parent output does not establish each child's PID or a native crash stack.
All three tests then passed separately against unchanged source: mask 8.23s,
text export 16.21s, project precision 59.93s. Those isolated successes do not erase
the failed full gate or establish its root cause. No assertion, production
renderer, panel, driver or machine setting was changed to obtain them.

At the time of this recorded run the PR remained a draft pending the unresolved
full-regression gate and final-head cross-platform checks. Native crash attribution and the other explicit scope
limitations above remain open; successful focused tests do not close issue #88.


## Current integrated-source follow-up

PR #583 is now formal. The [layer-time integration record](tracking88-pair-clock-integration-current.md)
and its source-bound evidence describe the latest parent/main synchronization,
complete model/core and host-library regressions, actual paired video/audio
controls, and exact remaining acceptance boundaries. Earlier results and failed
runs above retain their original source and scope; they are not final-head claims.
