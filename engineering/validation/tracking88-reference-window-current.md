# Full-resolution reference-window boundary (#88)

Status: geometric boundary correction and controlled measurements complete;
tracker and full3D acceptance remain incomplete. The existing point path,
forward/backward, photometric, RANSAC, holdout and extrapolation thresholds are
unchanged. No dependency, model, media, panel or native rendering code changed.

## Corrected boundary

Previously, a plane feature centre inside the ROI could have its full-resolution
reference LK window read outside the selected plane. Source reseeding now erodes
the rasterized plane mask before detection. Reference refinement intersects
decoded-image support with the retained reference-plane mask before erosion.
The seed radius14 derives from the unchanged21-pixel LK window, one
interpolation neighbour, one derivative neighbour and the existing5-pixel
Gaussian prefilter. For rectification, a Float32 plane-support mask is projected
into the current source image, eroded for its Gaussian support, and rectified
back. It is intersected with filtered reference support before the12-pixel
LK/interpolation/derivative erosion. UInt8 rounding is avoided because partial
interpolation coverage could round back to255. The public
[OpenCV5 source](https://github.com/opencv/opencv/blob/5.0.0/modules/video/src/lkpyramid.cpp)
identifies the Scharr derivative and interpolated gradient/image sampling;
no upstream implementation was copied. It is not a fitted error
threshold. The retained mask resets with each explicit manual reference reseed.

This only establishes the full-resolution reference-window boundary. Adjacent
pyramidal LK still uses its existing coarse search and may read outside that
support; the mask is not a claim of complete pyramid or physical-plane support.
An inaccurate predicted homography can still misidentify scene content. These
limits and the existing geometry/appearance/holdout gates remain necessary.

Two new actual-CV controls verify complete seeded windows after identity and
projective reseeding, and refuse a plane whose only detectable texture lies at
its boundary. All8 existing flow controls retain their assertions. All10 flow,
2 residual diagnostics,10 plate admission and2 calibration admission tests pass.
This24-control result is separate from full native/workspace regression.

## Fresh complete original-sequence measurements

Source/input hashes were frozen during each run and checked afterward. Both
original modes ran once per source. The original initial/manual request files
were copied byte-for-byte. Quality annotations were read only after both blind
outputs were sealed. Every automatic visible result is scored against its
original annotation; no corner/frame/error trimming or threshold change occurs.
Manual corrections and losses retain separate coverage counts. The original
15px maximum gate remains unchanged. These are known research sequences,
not unseen qualification data.

| Source/mode | Track | Automatic visible | Lost | Manual | First loss | Over15px | Maximum px |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Phone2300 / no corrections | point |57|2242|0|58|0|2.622753|
| Phone2300 / no corrections | plane |73|2226|0|74|0|5.742232|
| Phone2300 / manual every200 | point |943|1345|11|58|0|7.912377|
| Phone2300 / manual every200 | plane |651|1637|11|74|10|31.673471|
| Mousepad6946 / no corrections | point |96|6849|0|97|0|3.685786|
| Mousepad6946 / no corrections | plane |7|6938|0|8|0|2.398911|
| Mousepad6946 / manual every200 | point |3049|3862|34|97|5|16.205062|
| Mousepad6946 / manual every200 | plane |484|6427|34|8|0|11.963178|

The initial sample is excluded from automatic-error totals. The phone plane
still fails its original maximum gate; see the exact final counts above. Mousepad's corrected point
still fails on5 samples; the plane's low visible coverage does not qualify it
because its reported errors happen to remain below15px. The plane loses earlier
than the previous mousepad first-loss14 diagnostic. No failed observation is
converted to visible and no improvement in overall tracking quality is claimed.

The phone measurement runs the actual bounded prototype export for all2300
source frames. Mousepad remains the original6946-frame core-only diagnostic;
it exceeds the unchanged4096-frame prototype limit. Successful core measurement
does not waive that refusal or establish native import/export acceptance.

## Retained evidence and remaining work

The [archive](data/tracking88-reference-window-v3.zip) and
[manifest](data/tracking88-reference-window-v3.json) retain authored controllers,
exact changed source, original receipts and test logs. All members passed CRC
and byte-for-byte readback. Source media, annotations and track assets are
excluded; reproduction requires the previously attributed exact external inputs.

Final terminal results: controls exec`7c568b`, phone exec`1970f8`, mousepad
session26347/terminal`50f0a4`, admission exec`0fda00`, all exit0 and consumed.
Exit0 establishes completed measurements, not passed footage-quality gates.
No full native/workspace regression or new exact-head CI covers this increment
yet. Existing complete-run failures, original photographed failures and the
remaining #88/#75/#82 boundaries remain open. No unchanged retry, error-threshold sweep, source substitution or
annotation-dependent estimator selection followed.


Three implementations are retained separately. The initial radius13/source-mask
implementation omitted derivative support; its completed footage measurements
remain negative evidence, not final-source validation. The second implementation
added derivative support and source-space filtering. Final source further keeps
interpolation support in Float32, preventing UInt8 rounding from admitting a
partial footprint. Those are independently derived operator-support corrections,
not variants selected by measured corner accuracy. Every changed version had its
own preserved controls and two-mode measurements. No original attempt was erased.
The first archive retains its exact earlier source; the final archive retains all
three receipt sets and the final source. This boundary correction still provides
no claim that the tracker is ready for production or that #88 is complete.
