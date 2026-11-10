# One fixed MAGSAC candidate: rejected (#88)

The [selected first-loss phase evidence](tracking88-flow-phases-current.md)
showed that the original final homography extrapolated differently from its
prediction. This justified a separate estimator check; it did not identify a
fix. One declared MAGSAC fit was evaluated on the exact sealed correspondence
array from that diagnostic. It was rejected and is not product/tracker code.

The method is the existing OpenCV `USAC_MAGSAC` implementation. Its
[official USAC reference](https://docs.opencv.org/4.13.0/de/d3e/tutorial_usac.html)
describes MAGSAC scoring and sigma-consensus refinement. No implementation,
documentation prose, dependency, asset or runtime was copied or acquired.
No method/parameter search or retry was performed.

## Fixed procedure and actual result

Source `a6b060ccbba1d5ee6a93910441127ceb48f95f6a`, existing OpenCV5.0.0.93
and NumPy2.4.6, all tracked hashes and the selected sealed arrays were frozen.
The candidate received only146 original fitting correspondences. It did not
receive the46 heldouts or any corner annotations. The single fit retained the
original1.5px threshold,2,000-iteration ceiling and0.995 confidence, with one
CV thread. The controller does not open an annotation or media file.

After fitting, actual residuals independently recomputed119/146 inliers,
ratio0.8150684932. The library's returned mask matched that recomputation.
Held-out RMS was1.1761899600px; maximum was3.1493409984px. The unchanged
`check_plane_residuals` rejected it with the original numeric cause:

```text
plane held-out residual exceeds limit: heldout=46 rms_px=1.17619 maximum_px=3.14934 rms_limit_px=1.5 maximum_limit_px=3
```

The independent extrapolation gate was not reached after this refusal; it is
not counted as passed. Better fit support and RMS cannot override the maximum
gate. The candidate was not installed, tested with alternative parameters,
rescored against labels, or substituted for the original prediction/final fit.
Neither existing flow options nor rejection thresholds changed. No source
observation was converted from lost to visible.

This is a selected negative estimator result, not a new complete-video run.
The original phone2300 and mouse-pad6946 quality/coverage failures remain
unchanged. Any later adoption still needs an independently justified improvement
and complete original-sequence and native/full-regression qualification.

## Evidence

Original session3111 exited0; terminal chunk `eee383` was consumed after source,
HEAD and input checks. Only then was its source freeze released. Exit0 means
the planned measurement finished; the candidate itself failed admission.
One owned controller ran, with no subagent, foreign-process access, debugger,
cleanup or environment workaround.

The [archive](data/tracking88-magsac-fixed-fit-v1.zip) and
[manifest](data/tracking88-magsac-fixed-fit-v1.json) retain the authored controller,
original receipt and diagnostic log, consumed-session record, consumer and
collector. All6 members passed CRC and byte-for-byte readback. Archive
size296,791 bytes, SHA-256:
`8eb3ea3ce29ecc166ecc241dab6b5ba4a7b060b88cadfae29f079b94e2534c4a`.
The collector exited0, consumed chunk `85b4b8`, without a running session.
The private array's digest is retained; media, labels and derived arrays are
not redistributed. #88 and the complete 3D route remain incomplete.
