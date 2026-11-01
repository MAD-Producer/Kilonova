# Tracking #88: independent planar runtime binding

This increment follows `2c031293` on `codex/88-track-import-contract`.
It does not close #88 or qualify the entire 3D route. Android remains retired.
No panels, UI presentation, layout, third-party dependencies or external assets
were changed. Fixtures and geometric controls are independently authored.
The existing attributed builtin corner-pin kernel and checked-in video are reused.

## Implemented boundary

- Project JSON accepts schema **34 only**. Optional per-layer `tracking_plane`
  binds immutable raw observations to an explicit existing first builtin
  `corner_pin` instance on a flat visual layer. Null, audio, camera, adjustment,
  3D and model targets are refused. A projective matrix is never inserted into
  the affine parent/hierarchy matrices.
- `set_tracking_plane` is atomic and undoable through the normal core/MCP edit
  route. Its catalogue describes the complete required payload, including
  binding-or-null; omission is an error. Invalid binding edits preserve project,
  revision and history. Removal/reordering of a bound effect requires a valid
  final binding state in the same atomic batch.
- The saved binding selects attachment, track, effect, exact original source
  PTS/local-frame anchors, signed rational speed, TL/TR/BR/BL reference pixels,
  explicit crop/rotation, hold/linear interpolation, maximum source-tick gap,
  minimum confidence and error/authored-only missing policy. Ordinary corner
  parameter tracks remain independent manual corrections; no keys are baked.
- Each endpoint homography projects the reference corners in binary64. Arbitrary
  homogeneous scale/sign does not change the result. Linear interpolation acts
  on projected corners, never ambiguous homography coefficients. The explicit
  plate mapping converts current-minus-reference displacement into target source
  pixels. It is added after authored effect sampling, so normal parameter keys
  and evaluated parameter expressions remain corrections.
- Projective poles, crossings, collapse, nonconvex quads, numeric overflow and
  effect-range overflow fail explicitly. Geometry errors cannot be hidden by
  `authored_only`. That policy only handles unavailable/out-of-range, lost,
  low-confidence or excessive-gap observations. No extrapolation or repair is
  performed. Disabled effects are quiet. An ended clip does not demand samples
  outside its source range; hidden in-range sources still sample for matte/image
  references.
- Shared Scene sampling supplies the values used by normal render preparation.
  The actual builtin identity is selected through the existing constant, rather
  than the retired motion-library source identifier. No new compatibility name
  is introduced. Existing output-bound rules expand to the sampled quad.
- Errors identify binding/sample phase, object, attachment, bounded track name,
  frame where available, and source-index/state/gap causes. Existing worker
  diagnostics add request, operation and elapsed context after releasing guards.
  Default Info start precedes import/render work. Error reaches file and stderr;
  stdout remains JSON-RPC only. Healthy lookup adds no logging or filesystem IO.

## Executed evidence

The source-bound native controller in `tracking88-plane-native-v2` built and ran
actual FFmpeg-enabled `motion-studio`, not a successful mock. It admitted every
one of the checked-in original video's 72 native VFR/B-frame PTS samples through
`import_tracking`. Managed `.mstrack` bytes match the complete original document,
including whitespace and a manual-correction marker. The external solve was
removed before undo, correction edits and save/reopen.

A synthetic independently authored perspective sequence drives the existing
GPU corner-pin effect on a solid. Frame 0, frame 30 and frame 30 with a genuinely
keyed manual TL correction were captured. Sampled corner residuals against an
independent Python projective oracle are below 0.0001 pixels. Per-pixel silhouette
comparison excludes a fixed two-pixel edge band (this is not AA qualification):

| Output | Interior pixels | Exterior pixels | Mismatches |
| --- | ---: | ---: | ---: |
| Reference | 35,280 | 263,024 | 0 |
| Perspective | 25,681 | 272,616 | 0 |
| Manual correction | 25,665 | 272,630 | 0 |

All three images were visually inspected. Both undo/redo and readonly reopen
preserve the independent binding, two authored TL keys, immutable observations
and original managed document. A rejected absent-track edit is atomic. A strict
zero-gap fractional seek fails with phase/object/asset/track/frame/gap context;
restoring policy recovers. Both default-level file and Error stderr contain the
original failure, request ID and elapsed time. All stdout lines parse as matching
JSON-RPC replies. The host machine's missing Epic Vulkan overlay-manifest messages
remain unrelated external loader diagnostics; no suppression was added.

## Failed attempts and limits

Initial local checks found duplicate declarations left by an interrupted edit;
they were removed before the passing check. The first focused test compile found
an incorrect fixture include path. The fixture now uses the current builtin
manifest. Native v1 reached two actual captures, then correctly refused the test
controller's incorrect EffectAction tag (`op`, required `kind`). V2 corrects that
fixture payload and includes the ended-clip production guard. Original logs,
transcripts and partial images are retained; no assertion or tolerance was
relaxed to obtain passing results.

Local exact-source regression completed with model **232 passed / 0 failed /
0 ignored**, core **481 passed / 0 failed / 1 ignored** (the existing manual
large-document history benchmark), and selected host filter **50 passed /
0 failed / 0 ignored**. Full model/core tests include their doc tests. The
FFmpeg-enabled desktop check and provenance guard passed. No new ignore or
weakened existing assertion was added. These selected host controls are not a
full-host rerun or native-crash repair. The host filter includes 49 tracking unit
controls plus the existing `resource_tracking_is_runtime_only_and_profiling_keeps_it_enabled`
integration control; the first archive finalizer stopped before writing because
it expected 49 total. Its accounting was corrected after inspecting the completed
log, without rerunning tests or modifying any product assertion.


The versioned archive and source-hash closeout receipt record exact files and
executed commands. Previous native Naga/access-violation failures, the MCP test
compiler access violation and #82 strict P95 failures remain unresolved historical
failures, not repaired or accepted by these controls. No unchanged failed target
was rerun merely to obtain green.

Remaining #88 scope includes calibrated camera runtime binding, known-camera
reprojection and calibrated real footage, lens/distortion output behavior,
relink/cache/exposure integration and full maintained desktop/workspace plus
cross-platform CI qualification. This planar synthetic control is not camera
solve, automatic tracking, a real-footage match, or proof of arbitrary formats.

Archive: `tracking88-plane-binding-v1.zip`, 968713 bytes, 82 members; SHA-256 `fcc9d8aafd7bcb1d9bf6b130e52496403780c95a759e3c24450cf9b9d53bc242`. Every archived member was read back and byte-compared. Exact source hashes are in `data/tracking88-plane-binding-closeout.json`.
