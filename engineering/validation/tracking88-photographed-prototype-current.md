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

The periodic run has 11 explicit corrections per track. At index 1,200 the
annotated plane transform violates the prototype's existing area budget. That
correction is represented explicitly as missing, rather than weakening the
geometry guard. The first attempt instead passed this unsupported correction
and correctly failed admission during execution. Ordinary automatic frames are
not relabeled as manual corrections or excluded from error counts.

## Implemented constraint and controls

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

## Native and remaining scope

The existing native proof in `tracking88-prototype-current.md` qualifies the
previous measured output. Native proof of the newly constrained output must be
recorded separately before it is claimed. The photographed source itself uses
MPEG-4 Part 2, outside the current native VideoAsset codec set. No transcode is
silently substituted for original native admission; a future explicit derived
plate must retain its relationship to this source and verify its own identity.

The adjacent reference closeout ZIP/JSON preserve authored controllers, aggregate
real measurements, tool source captures, synthetic evidence and diagnostic logs.
They exclude original third-party video/annotations, photographs, derived image
captures, generated per-frame tracking/annotation payloads and copied permission
HTML. Numeric acquisition records and source URLs allow deliberate reproduction.

Remaining: useful photographed-motion accuracy and coverage, the new output's
native qualification, retained interactive Surface/cache behavior, atomic new
plate/solve replacement, full maintained regression and exact-head cross-platform
CI. Neither this measurement nor conservative missing output closes #88.
