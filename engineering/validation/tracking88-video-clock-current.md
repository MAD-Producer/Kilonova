# #88 original-video publication under mapped layer time

PR #581 is synchronized with parent `9ec5ee4d4e39b6873eb81e42616ba9b2141c27f8`. The parent
already integrates maintained main `9d085c0f`; the merge is conflict-free and
retains its current-only schema 36. No new product behavior or UI is authored.

## Actual native combination controls

One new test executes four bounded combinations against the existing original
VFR/B-frame CTTS fixture: 3/2 stretch, reverse, linear source remap, and source
hold. Each uses the actual FFmpeg decoder, worker, host and GPU. An independently
addressed unretimed project supplies the expected image at an explicitly
tabulated source-frame instant. The reference must differ from frame zero and
contain more than 100 visible pixels, excluding empty/static-image false passes.
Every mapped image must match that native reference exactly; no image threshold
or original-source PTS is weakened or rewritten.

For each combination, a valid container with changed bytes is refused without
project/revision/pixel mutation. Identical original bytes remain admissible via
direct replacement and batched relink. Both transactions keep the authored layer
clock, original tracking PTS and camera binding through separate undo/redo
steps. Save and a fresh Engine reopen retain the complete project and image.

The imported camera observations are deliberately constant to isolate the
footage clock. The test does not imply that a camera/null on its own property
clock automatically follows a different footage layer's source remap. This
continues the explicitly documented binding semantics.

## Verification and evidence boundaries

- New four-case native test: one passed
- Existing native tracked-video replacement/relink control: one passed
- Complete host library: 164 passed
- Complete tracking_relink and asset_registration targets: 15 passed
- Existing media video group: 13 passed
- Source hashes remained unchanged throughout every command

The two native tests run in separate processes because the existing file logger
is initialized only once per process. With MOTION_LOG unset, their actual file
logs contain pre-verification Info starts; original-cause Error records reach
both file and stderr and never contaminate stdout. The original native control
and its assertions remain unchanged apart from the new module declaration.
Both inherit the existing ffmpeg feature gate; the recorded native commands
explicitly enable it. A default desktop test invocation without ffmpeg does not
execute these video tests. The new four-case native test took 48.89 seconds in
the successful invocation, separate from rebuilding and other regression steps.

The model, core and renderer source trees are unchanged from the integrated
parent; its broader package/renderer results remain historical evidence of that
parent, not a newly executed full regression on this commit. No panel differs
from maintained main. No dependency, third-party fixture or product retry was
added. The new source snapshots, native images, failed attempts and raw logs are
in the [receipt](data/tracking88-video-clock-v1.json) and
[verified archive](data/tracking88-video-clock-v1.zip).

The first invocation failed to compile the newly added test because it named
the camera binding under motion_host's tracking reexports rather than the
motion_core model API. Only that type path was corrected; no assertion changed.
The following source also adds explicit batched-relink coverage. Its first
runtime invocation correctly refused camera interpolation: the new fixture had
changed its PTS clock from 48,000 Hz to 1,000,000 Hz while retaining a gap budget
expressed in the old ticks. The fixture now converts 4000/48000 seconds to the
new units with integer flooring and explicitly asserts that the physical bound
does not increase. No original timestamp, image reference, assertion or physical
tolerance was relaxed. The next invocation also caught the new fixture reusing
a pre-undo revision for starting relink; the protocol correctly cancelled that
stale request. The fixture now captures the current revision after undo/redo
before staging relink. Cancellation and request lookup behavior are unchanged.
All earlier failures and source snapshots remain archived rather than counted
as passes.

This is actual offscreen GPU capture, not retained-window Surface acceptance.
It covers the existing CTTS source and four explicit clocks, not all codecs,
nesting or source mutations. Full desktop/workspace regression, final-head
cross-platform CI, photographed tracker quality and the full 3D route remain
separate work. Android remains outside the maintained scope.
