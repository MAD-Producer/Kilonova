# #88 actual Surface and retained tracking front

The new Windows FFmpeg control presents through a real hidden native window and
services its owner thread while the existing engine worker renders. It uses the
real platform decoder, a video layer and an imported independent camera track.
The window, device and session stay attached through replacement, undo and redo.
No production code, existing assertion, panel, interaction, layout or style was
changed to obtain the result.

Unlike `render_preview`, the test's `CaptureUi` reads the already committed
preview front without sampling or rendering a new composition. The test also
checks actual successful Present replies, the presented counter and the worker's
`compositionRenders`/`compositionReuses` counters. These observations prove the
retained preview path in this controlled case, not physical display scanout.

The control verifies:

- A nonempty native video/camera frame at project frame 24
- Repeated presentation reuses the front without a composition render
- A source-mismatched solve is refused with unchanged visible pixels
- A valid new video/solve pair commits one revision and invalidates the old front
- Undo restores the exact old pixels; redo and reopening restore the exact new
  pixels, with successful subsequent reuse in each session
- Default Info starts, original error causes in file and stderr, no corresponding
  error leakage into stdout, and no routine Present start records

One focused test passed, zero failed, zero ignored, in 12.07 seconds. Its actual
native child exited with code 0 (`0x00000000`), PID 39204, elapsed 12060 ms. The
source-bound controller, including compilation, completed in 148.969 seconds.
The [receipt](data/tracking88-surface-v2.json) and
[36-member source/log/image archive](data/tracking88-surface-v2.zip) retain the
exact source identities, measured counters, captured fronts and original streams.
The 223221-byte archive was read back member by member before publication.

## Retained first failure and corrected test setup

The first new control incorrectly required an immediate reuse after a rejected
Media command and failed: composition render count 3 versus 2. The existing
dispatcher intentionally clears its retained stamp *before every Media command*,
independently of success. Its current safety behavior should not be changed to
satisfy that new test assumption.

The final new control instead checks the existing contract more completely:
the old front remains byte-identical immediately after rejection; the next
Present really redraws the unchanged project with equal pixels; repeated
presentation then reuses that front. Original failure source, PID/status, logs
and output remain in the archive. No pre-existing success condition or pixel
assertion was disabled or weakened, and production error/cache policy is unchanged.

## Scope and regression boundary

The preceding [complete package run](tracking88-desktop-package-current.md)
passed 1542 tests before this test-only addition. This new focused result is
separate; it is not presented as another full run on the augmented test inventory.
Final-head CI is required separately. Earlier native access violations remain
unexplained, rather than being labeled fixed by subsequent successes.

This case uses one pair of existing synthetic native video fixtures and one
camera binding. Whole-project rejection checks also remain in the preceding
paired-transaction tests; pixel equality alone is not used to prove that every
project field is unchanged. This case does not qualify every codec, nonlinear lens, point/plane
binding, audio output device, or the optional prototype's photographed tracking
quality. It adds no UI and makes no claim about a complete tracker/3D solver or
completion of #88. Android is retired and excluded.

## Subsequent point, plane and nonlinear video qualification

The [native video/lens increment](tracking88-native-video-lens-current.md) adds
actual Surface controls for point, corner-pin plane and nonzero radial/tangential
camera bindings with identical source video bytes. It fixes the ordinary 3D
video/lens admission gap discovered by that control and independently checks
queued RGBA/YUV video inputs. The narrower result above remains historical;
the follow-up records its own final source, measurements and remaining limits.
