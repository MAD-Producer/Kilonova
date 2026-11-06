# #88 photographed tracking measurement: acceptance failed

The developer tracker remains draft. A complete 2,300-frame photographed phone
sequence exposes insufficient coverage and incorrect visible plane results.
The source is the Zimmermann/Matas/Svoboda benchmark, with independently marked
corners. Its custom public-use statement, citation and redistribution limits are
recorded in `ATTRIBUTION.md`. Original media, annotations and derived images or
per-frame annotation data remain local, outside repository/product/CI packages.

## Input and conventions

- Original video: `phone_100fps.avi`, 7,193,010 bytes, SHA-256
  `c67202dc312ec945bed714c8c812ab6165ca526c1694a705a40eeb8a578b16b9`
- Annotation archive: `phone_gt.zip`, 474,577 bytes, SHA-256
  `d4e36ea6f58d11fcdaab2e604786401725059581c767c65b9923595f79567886`
- The source page is
  <https://cmp.felk.cvut.cz/cmp/demos/Tracking/linTrack/data/index.html>.
  Acquisition records retain exact URLs and the permission-page digest.
- All 2,300 original frames and PTS `0..2299` on the 100-tick/second clock are
  checked. No benchmark prefix, FPS inference or synthetic interpolation is used.
- Annotation files are exactly `img_00200.txt` through `img_02499.txt`.
  Frame zero maps to the first annotation. Source rows precede columns; the
  evaluator interprets the MATLAB convention as one-based and subtracts one
  when converting to zero-based x/y. This convention and independent manually
  marked corners do not establish subpixel metrology or an exact physical plane.
- Point seed: the first annotated mark. Plane ROI: independently selected
  `[160,180,340,210]`. Plane errors measure all four annotated marks, including
  extrapolation beyond that ROI. The benchmark describes almost-planar objects;
  the phone's physical surface need not satisfy one homography exactly.

## Measurements, including missing output

These are measurements, not passed acceptance. Automatic counts exclude the
initial seed and all explicit correction samples. Missing frames remain missing
and are not removed from the coverage denominator. Max errors describe visible
automatic results only and cannot compensate for poor coverage.

| Implementation/run | Track | Automatic visible | Automatic lost | Maximum error, px | First lost index |
| --- | --- | ---: | ---: | ---: | ---: |
| Published baseline, no edits | Point | 1,599 | 700 | 44.1607 | 1,600 |
| Published baseline, no edits | Plane | 8 | 2,291 | 4.3634 | 9 |
| Reference checks, no edits | Point | 57 | 2,242 | 2.6228 | 58 |
| Reference checks, no edits | Plane | 10 | 2,289 | 6.4448 | 11 |
| Reference checks, corrections every 200 frames | Point | 943 | 1,345 | 7.9124 | 58 |
| Reference checks, corrections every 200 frames | Plane | 434 | 1,854 | 57.0703 | 11 |
| Projective reference, no edits | Plane | 68 | 2,231 | 6.8199 | 69 |
| Projective reference, corrections every 200 frames | Plane | 915 | 1,373 | 56.6400 | 69 |

The periodic run has 11 explicit corrections per track. At index 1,200 the
annotated plane transform violates the prototype's existing area budget. That
correction is represented explicitly as missing, rather than weakening the
geometry guard. The first attempt instead passed this unsupported correction
and correctly failed admission during execution. Ordinary automatic frames are
not relabeled as manual corrections or excluded from error counts.

## Previous reference constraint and controls

New point/plane reference-image cycles constrain accumulated local drift. Only
an explicit edit resets the reference. Plane fit and holdout sets remain
separate, and all original fit/holdout thresholds remain unchanged. Adjacent
flow retains its photometric threshold. Long-baseline geometric refinement uses
the adjacent prediction at full resolution, avoiding coarse-level aperture
ambiguity; it does not assume unwarped long-baseline brightness is invariant.

Five optional real-CV controls exercise analytic translation, the small
projective-motion regression, locally consistent but unrelated images, point
and plane manual recovery, and preservation of adjacent appearance rejection.
The ordinary tools suite discovers 118 tests: 117 pass and the existing optional
native effect-loader test skips without `MOTION_EFFECT_VALIDATOR`.

`tracking88-prototype-synthetic-v4` passes the original unchanged 96-frame
analytic-video gates: 84 automatic frames per track, point maximum/RMS
0.164875/0.078385 px and plane maximum/RMS 0.087676/0.040635 px. Budgets remain
0.75/1 px. Occlusion, manual recovery and the separate 60-occurrence Error
file/stderr suppression check pass. These results qualify that synthetic case,
not the photographed sequence.

Earlier attempts remain recorded: a robust held-out outlier-selection experiment
did not establish real accuracy and was removed; applying an unwarped reference
brightness gate discarded valid motion; coarse reference pyramids rejected a
known two-pixel synthetic motion. The final implementation retains the original
holdout/RMS/maximum budgets and has a specific small-motion regression control.
No existing repository assertions or acceptance tolerances were changed.

## Projective reference increment

The current plane implementation first predicts a homography using only the
fitting partition's adjacent-frame correspondences. It rectifies the current
image into the initial/explicitly corrected reference image, then measures
reference residual flow with the original brightness and cycle checks. This
compensates patch shape changes that a translation-only reference comparison
cannot represent. Manual reseeding retains its original-source coordinate map.
The existing adjacent appearance, reference agreement, RANSAC and held-out
RMS/maximum limits are unchanged. Missing output remains missing until an
explicit correction; no automatic target reacquisition was added.

Windows with source padding are rejected, including interpolation neighbours.
An independently fitted held-out homography must also agree with the fitting
homography at all four requested ROI corners, within a new 3-pixel maximum.
Low residuals over a narrow strip cannot alone establish edge extrapolation.
This check cannot detect common model error or promise accurate extrapolation
outside the requested ROI. The benchmark corner oracle deliberately retains
those outside-ROI measurements and all missing frames.

The full 2,300-frame photographed v6 measurement uses the same source, seed,
ROI, ground truth convention and 11 explicit corrections per track. Point
results are unchanged. Plane automatic coverage increases from 434 to 915
frames in the corrected run, while 1,373 automatic frames are still lost.
Its maximum error remains about 56.6 pixels; 21 visible automatic frames exceed
15 pixels. This is still **failed real-motion acceptance**. Independent held-out
agreement and higher coverage are not proof that the result is usable.

Eight optional real-CV controls now pass, including the earlier five controls,
a 60-frame known perspective sequence with independent five-probe error below
0.1 pixels (both original and manual-reference seeds), narrow-strip residuals
that fail edge extrapolation, and refusal under severe clipping. Ordinary tools
regression again discovers 118 tests: 117 pass, one existing native effect-loader
test skips without `MOTION_EFFECT_VALIDATOR`. Existing assertions were unchanged.

Synthetic v5 retains the original 96 frames, 84 automatic frames per track,
occlusion/manual recovery and default-level Error file/stderr suppression gates.
Point maximum/RMS remain 0.164875/0.078385 pixels; plane maximum/RMS are now
0.021854/0.009265 pixels, within the unchanged 0.75/1-pixel budgets.

Native v6 consumes that exact new original and measured output through the
actual FFmpeg desktop executable. Native source/timeline admission, point and
plane bindings, undo/redo, all nine GPU pixel oracles, strict missing-sample
refusal, historical error preservation, save/reopen, default Info starts,
Error file/stderr and JSON-RPC isolation pass. Original SHA-256 is
`b5309a8e584f3d0547940608111dd8231bf58734f9a9c505de0d9b440e84519a`;
solve SHA-256 is
`88cc9e7843a1bdf50b03d05b69bbdc0b8b0ef8b1a8d1e710ed5da550e7d17d9b`.
This is Windows native synthetic qualification; interactive retained Surface,
the photographed codec and complete cross-platform regression remain separate.

`tracking88-projective-closeout-v1.zip` and its JSON receipt preserve the exact
measured source snapshots, all five serial validation phases, native protocol
transcripts/project/captures, authored synthetic media and aggregate real
measurements. The source base is `3f246093`, with explicitly recorded dirty source
hashes and normalized-LF hashes linking these changes to the subsequent commit.
The two earlier projective candidates are frozen separately, including the
coverage/error tradeoff before independent extrapolation checks. Third-party
benchmark media, annotations, images and per-frame payloads remain excluded.

## Native and remaining scope

`tracking88-prototype-native-v5` completed with exit 0 against committed code
`3f9a3cfe` and the newly constrained synthetic-v4 output. It rebuilds the actual
FFmpeg desktop executable and checks native original-source/timeline admission,
both bindings, seven source-time GPU captures, undo/redo, strict missing-sample
refusal, manual recovery with the historical error preserved, save/read-only
reopen, and a fresh-session capture. All nine pixel-oracle captures pass.
The new original SHA-256 is
`3d83869cf562e3708faac68567a85cb1c429c538286805139927c9b26b04fc06`,
and managed solve SHA-256 is
`bf02b43596c206cef0d9451e08a98409076ede774a8257a87141a260ad05cf55`.
`tracking88-reference-native-closeout-v1.zip` and its JSON receipt preserve
the controller, code hashes, process records, raw protocol/log channels, project
and captures. Original authored video bytes are referenced by verified SHA-256
into `tracking88-reference-closeout-v1.zip`, not duplicated. This qualifies
Windows native execution for that synthetic output, not real footage or a
retained interactive Surface.

The photographed source itself uses
MPEG-4 Part 2, outside the current native VideoAsset codec set. No transcode is
silently substituted for original native admission; a future explicit derived
plate must retain its relationship to this source and verify its own identity.

The adjacent reference closeout ZIP/JSON preserve authored controllers, aggregate
real measurements, tool source captures, synthetic evidence and diagnostic logs.
They exclude original third-party video/annotations, photographs, derived image
captures, generated per-frame tracking/annotation payloads and copied permission
HTML. Numeric acquisition records and source URLs allow deliberate reproduction.

Some abandoned intermediate experiments retain source hashes and numerical logs
without complete source snapshots; the final source and the failed coarse-level
synthetic source are preserved. These intermediate measurements are diagnostics,
not claims of fully reproducible final acceptance.

The atomic new plate/solve transaction is now tracked separately by #583; #584
adds its MCP prepare/status/finish/cancel/release workflow. Those increments do
not change the optional tracker's quality. Remaining here: useful photographed
accuracy and coverage, retained interactive Surface/cache behavior, full
maintained regression and exact-head cross-platform CI. Neither this measurement
nor conservative missing output closes #88.

## Follow-up: overconfident real-motion failure diagnosis

The [aggregate diagnostic](tracking88-plane-support-diagnostic-v1.json) and
[observer/controller archive](tracking88-plane-support-diagnostic-v1.zip) replay
the unchanged tracker on all 2,300 original frames with the unchanged ROI and
eleven manual corrections. They reproduce exactly 915 automatically visible and
1,373 lost plane samples, and maximum corner error 56.6400304460409 pixels.
External truth enters only the observer, never tracker fitting or decisions.

All 21 automatically visible samples above 15 pixels occur at source indices
1801 through 1822, after the explicit reference reset at 1800. Their median
internal confidence is about 0.898, higher than the 0.802 median among 855
samples below 5 pixels. The worst sample also differs from the corner-derived
oracle by 38.37 pixels at the four requested ROI probes: the 56.64-pixel corner
error is not explained solely by extrapolating outside the ROI. Fit features
cover a median 47% of the ROI convex-hull area in the over-15 group versus 85.5%
in the under-5 group. This association does not establish a causal threshold or
justify simply suppressing more output to pass acceptance.

The feature/oracle residual compares correspondences against a homography
derived from four annotated corners. It is not independent per-feature truth.
Two annotated corners at indices 1800 and 1801 are below the 480-pixel image;
all oracle measurements, including these, remain included. Key depth, repeated
patterns, distortion, offscreen extrapolation and annotation uncertainty remain
possible contributors, not established explanations for the failure.

This counterexample shows that independent fit/held-out agreement and low local
residual cannot certify target identity or turn the current confidence into an
accuracy probability. The next algorithm work must test common false matches
and ambiguous manual reference resets with independent appearance evidence,
while retaining this full-sequence quality gate. No ROI, truth, timeline,
correction schedule, threshold or existing assertion was changed. No third-party
video, annotations, diagnostic captures or per-frame payloads are redistributed.

## Follow-up: independent identity candidates rejected

The [complete identity experiment record](tracking88-identity-experiments-current.md)
tests persistent manual descriptor references and manual photometric feature
support separately, with independent analytic controls and all original frames.
Both retain all 21 severe errors while reducing automatic coverage, so neither
is adopted. Their frozen sources, actual logs and aggregate measurements are
archived; successful experiment execution does not change this failed quality
disposition or qualify a new native/full-regression gate.

## Follow-up: authored absence observation fixed

The [actual manual-absence qualification](tracking88-manual-absence-current.md)
reproduces and fixes false Error escalation after an explicit lost/occluded
correction. Full original real-sequence observations remain byte-identical;
only normal absence versus genuine automatic-failure observation changes.
Independent pixel/annotation diagnosis does not alter this failed quality gate.
