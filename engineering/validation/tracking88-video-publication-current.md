# #88 tracked-video replacement and relink admission

This increment qualifies original-video publication through two existing media
routes. It does not complete #88 or the 3D route. The parent increment is PR #580,
head `469412c5d61be1a8dfab54d534e0b2398beec2d1`. Android remains excluded.

## Changes and dispositions

- **VIDEO-1 / new publication guard.** `finish_media_replacement` previously
  called `VideoJobs::replace` without verifying attached tracking resources or
  original source identity. It now captures the ready candidate and immutable
  project/revision/document generation, verifies outside Session locks, then
  rechecks author state and publishes only that candidate. A same-container
  stream switch requires a new solve/source transaction; equal file hashes,
  dimensions and timestamps cannot establish the identity of two visual streams.
  The old route is evident in the captured parent source. Neither attempted
  pre-change native baseline established a successful end-user reproduction;
  their early failures are recorded below rather than relabeled as defect proof.
- **VIDEO-2 / exact task identity.** `VideoReplacementCandidate` captures the
  actual task Arc, staging identity and probe metadata. Publication checks these
  under the existing task guard before taking prepared resources. Releasing and
  recreating a request with the same string cannot reuse a predecessor's proof.
  Unit tests exercise recreation, metadata changes, cancellation and recovery.
- **VIDEO-3 / actual original clock.** Both direct replacement and batched relink
  verify original solve bytes, source SHA-256, descriptor consistency and native
  sequential decode to EOF against original rational PTS and raw dimensions.
  They use the same native verifier as original tracking import. Rounded preview
  timestamps, inferred FPS and proxy output are not admitted as clock evidence.
  Source bytes at the current path are rechecked after native decoding.
- **VIDEO-4 / Windows and project-path contracts.** Native tests caught two
  implementation defects: comparing canonical and noncanonical root spellings,
  and putting a private staging path into a project which requires `assets/`
  paths. The final implementation obtains a relative path from the owning media
  task and keeps the proposed project metadata separate from its staged reader
  location. Project validation and containment checks remain enabled.
- **VIDEO-5 / observations.** Info starts precede source IO and native decode;
  Error causes include request, tracking/source IDs, phase and elapsed time.
  The existing replacement commit/sample messages are deferred until Session
  unlock. Actual default-level tests check file logs, Error stderr and stdout
  isolation. No Present polling, UI changes, product retries or timeouts added.
- **VIDEO-6 / existing behavior.** Untracked media and audio retain the existing
  publication route. Superseded author state remains cancellation. A refused
  direct replacement leaves its prepared task available for explicit cancellation
  and release; this is exercised before a valid replacement. No silent unbinding,
  solve rewriting or changed-source fallback is introduced.

## Final-source evidence

`tracking88-video-regression-v2` completed all ten steps with exit 0. Its report
captures source hashes before execution, checks them between commands and after
completion, and preserves every changed Rust source file. Selected suites:

| Command scope | Passed |
| --- | ---: |
| motion-media lib `video::` | 13 |
| motion-host lib `tracking` | 51 |
| motion-host lib `ops::media` | 15 |
| motion-host integration `tracking_relink` | 7 |
| motion-host integration `asset_registration` | 8 |
| Desktop native tracked-image test with FFmpeg | 1 |
| Desktop native tracked-video test with FFmpeg | 1 |

The stream-identity test matches both host filters, so these rows describe command
executions, not disjoint test populations. Desktop FFmpeg check, provenance and
style-consumer checks also passed. This is not a full desktop/workspace run.

The video test uses the existing original `video-ctts.mp4` and checked-in native
timing manifest. Tracking samples retain that manifest's complete PTS sequence
and timescale, and original import independently checks native decoding. A valid,
independently authored trailing MP4 free atom changes encoded source identity
without damaging decoding; both publication routes must reject it and preserve
project/revision and warm native pixels. This video case changes encoded length;
the earlier PNG regression separately covers equal-length changed image content.

Identical source bytes under another path must work through both routes. The test
checks nonempty native video output, exact warm pixels, unchanged camera and
tracking resources, undo/redo, saved project reopening and byte-exact original
solve retention. Repeated captures exercise decoder-cache changes. They create
offscreen capture renderers and do not establish retained interactive Surface
texture/cache correctness. Default Info is used with `MOTION_LOG` unset.

## Retained failed invocations

All records are retained unchanged in the adjacent evidence archive.

| Invocation | Result and disposition |
| --- | --- |
| video-baseline-v1 | Test fixture's clip exceeded actual source interval; corrected the authored timeline before testing publication |
| video-baseline-v2 | rustc exited `0xc0000005`; no unchanged rerun, no product regression proof, compiler fault unresolved |
| video-native-v1 | Test helper assumed synchronous video capture; corrected it to wait only for the exact documented pending response, with a deadline and immediate failure for every other cause |
| video-native-v2 | Rejected on mismatched Windows root path spellings before the expected fingerprint error; corrected production candidate path handling |
| video-native-v3 | Rejected private staging path during project validation before the expected fingerprint error; separated staged IO location from managed project path |
| video-native-v4 | Passed native video, default file/Error stderr and stdout-isolation checks before the final unit-test-only edits |
| video-regression-v1 | New unit test used `unwrap_err()` on a result whose success type lacks Debug; corrected test extraction without changing assertions or adding a product derive |
| video-regression-v2 | Final selected regression accepted; source-bound report and raw outputs retained |

Existing Epic overlay Vulkan loader errors remain in raw stderr; they were not
filtered or repaired. Earlier compiler/native crashes, frozen external calibration
bit-exact repeatability failure and #82 performance/coverage failures remain
unresolved; these selected successes do not establish their repair.

## Remaining limits

- Cross-platform CI for this new increment and complete maintained desktop/
  workspace regression remain pending
- Retained interactive preview cache qualification and original-plate identity
  after out-of-band source edits at reopening remain pending
- A deliberate new plate plus new solve transaction, solve/lens replacement and
  the 2D/planar tracker prototype remain pending under #88
- Native success here uses one CTTS fixture; unit stream-identity refusal is not
  an independently decoded multivideo-stream fixture or every codec/timebase
- Verification is a bounded acquisition check, not a permanent filesystem lease
  against arbitrary external workspace mutation after verification

The closeout JSON records archive SHA-256 and per-member digests. Compilation,
selected test counts and this disposition document do not close the issue.
