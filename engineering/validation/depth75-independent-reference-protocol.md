# Independent sensor-depth evaluation protocol (#75)

This protocol is fixed before dataset acquisition or any model predictions.
It defines an initial spatial measurement, not domain certification, automatic
quality tiers, temporal-transition acceptance or issue completion. The source,
authors, release and license are recorded in `ATTRIBUTION.md` and `NOTICE`.

Use the official TUM RGB-D 2012 `freiburg1_desk` archive, pin its exact SHA-256
and review sequence-specific notices before inference. Retain local inputs
outside version control. Color and sensor-depth pixels are already registered;
do not independently undistort them or apply another depth-scale correction.
Sensor inverse depth in inverse metres is `5000 / raw_u16`; zero is missing.
This is a noisy sensor reference with asynchronous capture, not exact geometry.

Associate RGB and sensor timestamps with maximum absolute difference 20 ms,
deterministically prefer the smallest absolute difference, then earlier RGB
and depth timestamps, and use each frame at most once. Sort the resulting
associations by RGB time and take the first 32 consecutive pairs. Selection
must depend only on timestamps, before reading predictions or residuals.
Retain association differences and exact source identities for all 32 pairs.

Measure explicitly identified CPU ONNX and WGSL executions in the fixed order
266, 392 and 518 preprocessing resolutions, CPU before WGSL at each resolution.
The RGB/depth source geometry remains the complete registered 640 by 480 image.
Keep requested and actual model/runtime/plan/provider identities and all
attempts, failures, warm/cold status and cost components. No other backend,
frame selection, model procedure, retry or relaxed error bound may substitute
for a failed attempt. Verify existing same-weight numerical equivalence
references at each resolution before drawing cross-backend quality conclusions.

For each exact execution, fit one positive affine mapping from relative model
depth to inverse metres using every valid sensor pixel in the first eight
pairs. Use stable centred online covariance. Constant/nonfinite data or a
nonpositive fitted slope fails explicitly. The remaining 24 pairs are held out;
never refit, trim residuals, clip predictions or normalize each frame separately.
Reject nonfinite predictions even at missing sensor samples. Record valid and
missing counts separately for every frame.

Report pixel-weighted RMS, mean absolute and maximum inverse-depth errors.
Dimensionless normalized errors divide by exactly one inverse metre for all
frames, models and backends. A data-dependent extreme sensor value must not
inflate the normalization denominator. Aggregate RMS from the sum of squared
residuals and total valid pixels, and maximum over every residual. Values may
exceed one and must not be clamped. No pass threshold or claimed tier is
inferred merely from a smaller error than another candidate.

`tools/depth_reference_metrics.py` contains independently constructed affine,
outlier, missing-value and invalid-input controls. These analytic controls must
pass before model predictions are generated; they are not a learned-model
quality result. The original failed native command and subsequent complete
70-test supplement remain separately scoped evidence.

Continuous-video transitions, motion-compensated temporal comparisons,
complete process/weight/activation memory and longer thermal behavior need
their own fixed protocols and real observations. This initial reference does
not satisfy those remaining acceptance items.
