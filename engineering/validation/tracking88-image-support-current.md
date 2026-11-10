# Original phone corner errors and image support (#88)

Status: diagnostic scope resolved, original quality/coverage gates still failed.
This observation partitions the already measured original2300-frame outputs by
whether an annotated corner is inside the decoded640x480 image. No corner is
removed from the original15px maximum-error gate, and no tracker is rerun or
selected by this partition.

The [original data source](https://cmp.felk.cvut.cz/cmp/demos/Tracking/linTrack/data/index.html)
defines four hand-clicked anchor coordinates per image, first row=image row,
second row=image column. The already recorded zero-based conversion and exact
annotation ZIP hash are unchanged. The source describes almost-planar objects;
this observation supplies no physical coplanarity or camera calibration proof.

## Complete original output partition

Every automatic visible sample and all four original corners contribute. Manual
edits, initial sample and automatic losses retain their original separate counts.
The final reference-window output is identified by its actual sealed SHA-256
`a1e56f9639245923f279d59058c5915666ecd18fddf40ad98842a44c2b9c3053`;
the comparison output is the previously sealed original periodic-manual run.

| Measurement | Original tracker | Final reference-window source |
| --- | ---: | ---: |
| Automatic visible / lost | 915 / 1373 | 651 / 1637 |
| Samples failing original15px maximum | 21 | 10 |
| Failing samples with an in-image corner above15px | 0 | 0 |
| Failing samples with violations only outside the image | 21 | 10 |
| In-image / outside-image corner observations | 3603 / 57 | 2572 / 32 |
| In-image corner maximum pixels | 10.996743755610545 | 9.209216162463523 |
| Outside-image/original overall maximum pixels | 56.6400304460409 | 31.673471216421067 |

This locates these original failures at extrapolated, outside-image corners.
It does not prove that those annotations are wrong, that visible ROI pixels
identify the intended physical plane, or that the predicted map is correct.
The earlier ROI-probe and common-pixel disagreement remains. An image-support
partition cannot repair it or waive the full corner/coverage acceptance.

## Sparse source-image geometry

A separate observer decodes fixed original indices0,1800,1801,1810,1821,1830
with the previously pinned FFmpeg, without tracking or changing source timing.
Local overlays show the same original ROI, annotation corners and historical
prediction. At1800,1801,1810,1821 only two annotated corners are in the image;
at0 and1830 all four are. The annotation-derived projected ROI remains partly
visible: approximately87.9%,83.8%,85.0%,92.6% at those four partial-corner frames.
These percentages are geometric polygon clipping, not verified physical
visibility, occlusion masks, correspondence confidence or an acceptance score.
All decoded images and overlays remain private external validation artifacts;
none enters the product, repository or public evidence archive.

The independently inspected frames show that the selected ROI covers keypad
detail while the anchors enclose a larger phone region. Visual appearance alone
does not establish whether raised keys, body geometry, lens distortion or
off-image annotation uncertainty causes the disagreement. Additional physical
constraints are needed before changing the estimator on that basis. Previously
rejected initializer/window variants are not repeated, and outside-image corners
are not silently discarded or converted to successful tracking.

## Evidence and boundaries

The [archive](data/tracking88-image-support-v1.zip) and
[manifest](data/tracking88-image-support-v1.json) retain authored observers and
aggregate receipts only. CRC and every member's byte-for-byte readback passed.
No upstream implementation, original images, overlays, annotation arrays or
track payloads are redistributed.

The first sparse observer failed before output creation/decode because its
checkpoint reader used Windows default GBK on UTF-8 JSON (terminal`50cbba`,exit1).
The corrected explicit-UTF-8 observer completed exit0, terminal`d63db3`; the
standard-library complete partition completed exit0, terminal`5c96ae`. Both
terminal results are consumed. Product source remained `eb9b4825` and input
hashes were unchanged. No product, panels, assertions or thresholds changed.
This is not native regression, a tracker quality fix, or completion of #88.
