# Tracking #88: authored absence is not an automatic failure

The prototype previously reset its counter only on an explicit manual
lost/occluded sample. Subsequent inactive samples counted as automatic failures
and generated an Error at occurrence 60. A real CLI control reproduced this
before the fix, contradicting the documented normal authored-absence policy.

## Change and preserved boundaries

One bounded observation state per track/run now remembers authored absence until
an explicit visible correction resumes failure observation. Each manual state
resets the streak. Failed automatic tracking after a visible correction still
counts, including an unsuccessful reseed: first Debug, 60th Error once. The
Error now identifies source index, source identity, track, phase and cause.

This changes observation classification only. Subsequent samples retain their
existing lost visibility. Retained primary automatic causes are not cleared.
No optical flow, fitting, confidence, residual threshold, input or existing test
assertion changed. No UI, Rust runtime, dependency or restart was added.

## Actual verification

`validate_manual_absence.py` executes the actual optional tracker CLI against
340-frame locally authored encoded video at default Info, for both lost and
occluded tracks:

- Authored absence at zero produces no Error or retained fault, with complete
  unchanged missing samples and empty stdout
- A partial automatic-failure streak is interrupted by manual absence; after
  visible corrections each track emits exactly one Error at indices 180 and
  280, suppresses subsequent reports, and preserves the first genuine cause at
  frame 21; file and stderr Error records agree exactly, with empty stdout

The first control failed against the old implementation with the expected false
Error. After the fix, both complete CLI cases passed. Two ordinary CI regressions
cover durable manual state, per-track isolation, reset, recovery and suppression.

Frozen final source executed the complete tools suite: **120 discovered, 119
passed, one existing optional native effect-loader skip**, plus all eight
existing CV controls and unchanged 96-frame synthetic/80-frame flat video gates.
Both complete 2,300-frame photographed modes were rerun. Their entire
`tracks.mstrack` files remain byte-identical to the earlier baseline:

| Mode | Observation SHA-256 |
| --- | --- |
| No corrections | `4181647eae4a4d8e6acc2fb0e6ccf51395cb4141a7a71102e20df96552a56f9b` |
| Periodic corrections | `5dd567b7f1ec994a4120700fe529abdf97fdc27ffd7c10f327751ab91bc68dfa` |

The real authored-lost interval at 1200..1399 no longer emits a plane-failure
Error. Genuine failures elsewhere still escalate. Default Info start, Error
file/stderr equality and stdout isolation were checked on actual real CLI output.

The first aggregate validator stopped because the CV environment lacked an
existing PyYAML path and the new tool needed inventory declaration. The inventory
was completed and existing local PyYAML reused; no package was installed or
assertion weakened. V2 passed all four commands, then its final audit looked in
a v1 path. A separate V3 audit verified the executed v2 artifacts and frozen
sources without rerunning passing gates. Both failed receipts are retained;
v1/v2 are not described as successful end-to-end runs.

## Independent motion/input diagnosis

A read-only replay retained the original observation bytes. At frame 1801,
116 adjacent-flow measurements passed local guards, but differed from the
corner-derived incremental projection by a median 11.75 px. Image-patch median
absolute error was 2.87 under the measured transform versus 53.74 under the
corner-derived transform (98 and 88 valid patches respectively). This is not a
new quality gate or independent per-feature truth.

The upstream [data description](https://cmp.felk.cvut.cz/cmp/demos/Tracking/linTrack/data/index.html)
associates annotations with named JPEGs and describes AVI as a quick overview.
The original 2,300-image archive was acquired locally and recorded in
`ATTRIBUTION.md`. Comparing all names with AVI found no established fixed shift.
Of 704 nonzero nearest-image offsets, 679 select byte-identical JPEGs; the other
25 improve raw downsampled MAE by at most 0.487. No frame mapping was changed.

There are 687 identical adjacent JPEG pairs. Their annotations sometimes differ
(maximum 11.40 px, two pairs above 5 px, none above 15 px). This establishes
finite annotation consistency limits, but does not explain or excuse 56.64 px
tracker error. The identical pair at 1799/1800 also has identical annotations.
No truth, failing frame, ROI, correction schedule or quality threshold changed.

## Evidence and open work

The adjacent [receipt](data/tracking88-manual-absence-v1.json) and
[archive](data/tracking88-manual-absence-v1.zip) retain red/green CLI controls,
old/final sources, aggregate failures/final audit, actual raw logs, synthetic
summaries, complete photographed numeric summaries, original-image acquisition
identities, and diagnostic controllers/aggregates. Third-party media, images,
annotations, permission HTML, local captures and per-frame payloads are excluded.

Photographed quality remains failed and #582 remains draft. This fixes a false
fault signal; it does not certify identity or establish the cause of the shared
model error. Retained Surface/cache qualification, maintained full regression,
final-head CI, #85/#86 and the broader 3D route remain open. Android is excluded.
