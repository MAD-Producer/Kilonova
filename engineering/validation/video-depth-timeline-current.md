# Indexed video depth authoring and live admission — issue #87

Follow-up implementation and acceptance of bounded frozen temporal export are
tracked in [frozen-temporal-depth-current.md](frozen-temporal-depth-current.md).
The refusal described below records this increment's original boundary.

This increment adds independently calibrated indexed source intervals and bounded
live playback retention. It does not complete #87, real-codec video acceptance,
or frozen temporal depth export. No Android work or panel/style changes are included.

## Authoring and admission

- `Layer.video_depth_calibration` is an optional `VideoDepthTrack`, mutually
  exclusive with the existing single-sample `depth_calibration`.
- `video_depth_calibration` inserts or replaces one exact source PTS through
  ordinary edit transactions. All other samples remain unchanged. Existing
  `depth_calibration: null` clears either calibration mode.
- Each sample contains independently saved calibration, its RGB fingerprint,
  model/geometry interpretation, reference camera/world transform and indexed end.
  The source PTS is nonnegative microseconds. Intervals are half-open, sorted and
  nonoverlapping; there is no interpolation across relative-depth samples or gaps.
- A track holds at most 4096 metadata samples per layer. Deserialization, edits,
  original asset/display size, registered video range and frame count are checked.
  Save/reopen, undo/redo and cross-project asset remapping preserve this data.
- `Session::video_depth_calibration_command` derives the end from decoder staging,
  rather than trusting a backend or client-supplied timestamp. Host transactions
  validate every surviving new sample against the installed actual PTS, end, RGB,
  interpretation, source object and composition. Failure rolls back the edit/history.
- Reference transforms are sampled from the authored camera without observation
  views or rendered pixels. A separate reference-only scene omits video tracks so
  an uncovered interval does not prevent authoring its own calibration.

## Playback ownership and budgets

- Completed indexed results retain their original display RGBA and scalar planes
  within the current document generation/revision, root, composition and memory policy.
  Cache validity is separate from readiness at the current source interval.
- The live cache shares the existing 192 MiB combined scalar/RGBA budget and
  `MAX_LAYERS` sample cap. Old completed video samples are evicted in completion
  order. Static results and other objects' current render samples are pinned.
  Oversized incoming or pinned-set refusals preserve prior admissions atomically.
- Selecting a saved interval can stage its explicit model and geometry automatically.
  Repeated polling does not resubmit the same current request. A pending request
  may survive timeline movement only inside the same authored indexed interval;
  independent staging still verifies the decoded PTS/end. Other intervals cancel it.
  The legacy single-video-sample API retains its exact-frame invalidation contract.
- Re-requesting an interval supersedes only that object's interval. A completed
  result for another object or source interval cannot substitute for the selected one.
- Healthy polling does not construct a pinning list. That work occurs only when
  accepting a completed result. There is no per-Present start/done logging.
- Worker Info start remains visible before decoder/backend entry and now includes
  the source object alongside request, asset, revision, target and phase. Existing
  log prefix assertions are preserved.
- A missing calibration interval refuses native output. A missing current admission
  remains pending; flat geometry and another frame's colors are never substituted.
  Frozen export explicitly refuses indexed video depth until temporal snapshot
  ownership is implemented, including feature-disabled validation.

## Verification

The source-bound `video-depth-timeline-green-v3/accepted.json` records:

| Selection | Passed | Failed / ignored |
| --- | ---: | ---: |
| Host library | 112 | 0 / 0 |
| Video admission/native rendering | 19 | 0 / 0 |
| Existing depth-session workflows | 17 | 0 / 0 |
| Indexed-track model | 4 | 0 / 0 |
| Core calibration and layer clipboard | 16 | 0 / 0 |
| **Total selected tests** | **168** | **0 / 0** |

The same source-bound run passes host compilation without depth, desktop
compilation with FFmpeg/depth, style-consumer validation and provenance validation.
This is affected-path acceptance, not full-workspace or remote CI acceptance.

An additional same-asset/different-object fixture passes the exact authoring test
in `video-depth-timeline-object-guard-v1/accepted.json`. Its controller verifies that
the only change from the full gate is that test file; all product sources match.
This is supplemental execution of an existing test, not another unique test count.

The actual native test produces 12 32×32 PNGs: three indexed colors at each of
Unorm8, Unorm16, Float16 and Float32. Independent blue-plane occlusion and each
frame's own near color are asserted. A controlled blocked backend proves that
same-interval polling submits once. Returning to cached fractional VFR times after
removing the original file reproduces accepted pixels without another decoder open.
An uncovered interval refuses capture and preserves the prior output bytes.
`contact-sheet.png` in the archive was visually inspected. These are controlled
decoder/native GPU results, not a trained-model or real-codec movie claim.

## Evidence and attempt dispositions

[Archived evidence](data/video-depth-timeline-native-v1.zip), SHA-256:
`90b16c6f032fb65b65c49016cbcedc43e931727d7c46cb6f89558a7202ab949e`

The archive has 130 entries, including manifests, source snapshots, commands,
source hashes, raw logs, PNGs and prior attempts. Every entry was read back and
checked byte-for-byte by SHA-256.

- Red v1: OS error 1455 during compilation; no product regression was established.
- Red v2: an invalid video-clip fixture failed validation; not a product defect.
- Red v3: the valid core fixture reproduced the absent `video_depth_calibration`
  command. The later green run covers authoring, storage and history behavior.
- Green v1: 151 selected tests passed before automatic staging/pinning was added.
- Green v2: 18 passed and one existing default-level log-prefix assertion failed.
  The new object context had interrupted that prefix. Product log ordering was
  corrected; the original assertion was not changed. This attempt is not accepted.
- Green v3 and the supplemental object-guard run are the accepted evidence above.

PR #542 was independently merged as `a051a7c6f055fed305ce4af780a71d34f7abb9ba`
after all five required exact-head checks passed. The actual squash message was
verified as Chinese. Its main source tree equals this increment's tested base
`49e74cd74193a3de2637b1dd6db0db44ab38f632`.

## Remaining #87 work

Real-codec calibrated video output, frozen temporal export and sustained playback
throughput remain open. A bounded live cache is not durable ownership of a whole
movie, and 60 occurrences must not be interpreted as one second. Complex effects,
masks/mattes, nesting, exposure sampling, intersecting transparency, DOF, normative
comparisons, current-main full-workspace regression and maintained-desktop
cross-platform acceptance also remain open. Later optional nodes/RT/volumes and
retired Android validation are outside this delivery scope.
