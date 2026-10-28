# Static image depth through camera playback

Issue: #87. Integration base: `90452106669519fe91b803238638024f15ccb306`.
The archive records the exact implementation and test source hashes before commit.

## Deliberate functional change

Only an already admitted static image with owned original pixels may survive
changes of the composition clock. Pending worker jobs still require the exact
request frame. Time changes cancel their tickets and clear installed state;
late results cannot become reusable admissions. Video samples retain exact
time guards and cannot use this static-image rule.

Every retained entry independently checks document generation, editing revision,
composition, project root, decoded-memory policy and registered source mapping.
Invalid entries release scalar and image allocations even if the last worker
ticket has already been cleared. Explicit cancellation releases all admissions.
A new request supersedes that source immediately, preserving other valid sources.
Saved calibration still verifies actual prepared RGB/model/interpretation.

This is a functional change to the old clock-invalidates-all rule. Four existing
tests were updated intentionally: already admitted static geometry can render or
freeze at another camera time; explicit cancellation still refuses output.
Original canceled/pending/late-ticket checks remain, and accepted previous PNGs
are checked unchanged when cancellation prevents a replacement. Existing final
pixel, source-change, document, revision, rollback and video assertions remain.
These changes are supported by a separately reproduced starvation regression;
they are not assertion removals to accept unchanged product behavior.

## Actual rendering evidence

- Six sample times: frames 0, 1, 2, 3, 15 and 30
- Actual animated camera keys, with independent first-frame near/far visibility checks
- Fresh headless live-session PNGs equal independently verified frozen geometry at every sample
- First and last actual images must differ, preventing an empty or unanimated oracle
- Completed inference count remains exactly two (authoring and post-commit admission)
- Old worker installed state stays absent after the first frame change
- Six live PNGs, six frozen PNGs and six exact comparison reports retained
- Full FFmpeg media library rerun, including four-precision depth and imported-camera exports

The first green attempt failed the strict motion assertion because the test
fixture set values without enabling camera animation. It is retained as a failed
gate, never accepted. The fixture now enables animation and additionally checks
visible near/far geometry at frame zero; the motion assertion was not weakened.
The original red attempt proves frame-advance starvation, not a visual baseline.

## Validation and limits

- Host library: 109 passed; depth session: 17 passed; video: 13 passed
- Full FFmpeg media library including manual timing: 56 passed
- AI-disabled host and FFmpeg/AI desktop compilation passed
- Provenance and panel style checks passed
- No failed/ignored tests; source hashes unchanged after every command
- Four depth MP4s and four imported-camera MP4s, each with all 31 decoded frames checked

This is selected affected-target acceptance, not final full-workspace or remote
cross-platform acceptance. Live camera fixtures use Unorm8; the media regressions
exercise Unorm8, Unorm16, Float16 and Float32 working precision with explicit SDR
output. There is no new frame-rate, performance or model-accuracy claim.
The original-image ownership and GPU eviction/quality tests are documented in
[owned-depth-image-current.md](owned-depth-image-current.md).
No UI, panels, styles, external assets or dependencies change. Android is retired.

Calibrated-video geometry and temporal calibration, complex effect/mask/matte/
nesting/exposure depth, intersecting transparency, DOF, normative comparisons,
final current-main full-workspace regression and maintained-desktop CI remain open.

Archive: [static-depth-playback-native-v2.zip](data/static-depth-playback-native-v2.zip).
SHA-256: `1827072b1ed7b43718d906dfce41f5ec931fb79e4be571bd4f3af3d4ca695884`. Every entry was read back and byte-compared.
