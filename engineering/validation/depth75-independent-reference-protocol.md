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

## Acquisition status

The single original acquisition completed at source `2fea6a3e`, with unchanged
authored sources, exact Content-Length 344,011,403 bytes and archive SHA-256
`e983d6830916e66dc4a46a71368046b149b283de87769690e7aa4e0b9483530c`.
All 1,215 entries passed bounded path/type/size metadata checks; no separately
named sequence license/notice entries were present. The official benchmark's
default license remains the recorded source. Timestamp-list identities are
retained privately; no upstream lists, PNGs or sensor arrays are redistributed.
The verified seven-member evidence archive is identified by
`data/depth75-reference-acquisition-v1.json`.

Four analytic unittest cases passed before acquisition, covering the known
affine oracle, fixed physical normalization under an extreme sensor sample,
pixel-weighted aggregation and nonfinite predictions at missing sensor pixels.
No actual images have been decoded and no model predictions or learned-model
quality measurements have been made in this acquisition stage.

`tools/depth_reference_inputs.py` implements bounded exact-nanosecond timestamp
parsing and the fixed nearest unique association order. Six analytic unittest
cases passed for submicrosecond ordering, competing matches, tie ordering,
the inclusive 20 ms boundary, first-32 selection and malformed inputs. Selection
reads timestamp identities only; image content cannot alter the selected set.

## Prepared input identities

At `4ddb2e24`, the original preparation sealed the complete 32-pair selection
before importing the decoder or reading a PNG member. All 64 selected PNGs
decoded once at the registered 640 by 480 extent: RGB8 channels and original
16-bit monochrome sensor values. The first eight pairs remain calibration;
the following 24 remain held out. Maximum timestamp difference is 14,617,000 ns,
within the original 20 ms limit. No input was replaced or selected by content.

The original PNGs, decoded RGB and little-endian sensor arrays have retained
SHA-256 identities. All 64 local output identities were independently read
back before producing `data/depth75-reference-inputs-v1.json` and its verified
ten-member evidence archive. Aggregate sensor coverage is 7,657,585 valid and
2,172,815 missing pixels. Missing values remain missing; no residual-based
pixel filtering is authorized.

The existing separately attributed OpenCV 5.0.0 / NumPy 2.4.6 environment was
used for CPU PNG decoding with one decoder thread and OpenCL disabled. RGB
channels are retained directly without ICC/display conversion. Treating these
sensor-encoded RGB8 channels as the model's expected RGB8 input is an explicit
evaluation assumption, not color-management certification. No model inference,
quality result, quality tier or temporal-stability conclusion is established
by this preparation.

## Actual inference probe

The optional `motion-host` example `depth_reference` is an independently
authored caller of `Jobs::start_measured`. It requires the exact 32-row role,
order, geometry and source-byte identities, preserves each relative float32
output privately and records actual execution identity and worker/caller costs.
All 32 requests must complete without cache hits, restoration or discarded work.
It writes a bounded partial report on failure, preserves the primary cause,
uses the existing default file/stderr sink and keeps stdout empty. It does not
fit sensor values or make a quality/admission decision.

At `47540f1c`, both source-identity/role controls passed, with zero ignored or
filtered tests, and the actual executable built successfully. The original
external controller then failed on a Python `Path.with_file_name` typo before
invoking the CLI. That failed command remains failed. A second external driver
corrected the method spelling and reused the exact completed binary/tests;
neither native command was rerun and no product or fixture bytes changed.

The actual first CLI invocation used a separate one-byte-tampered RGB copy.
It refused the frozen-identity mismatch before submitting inference, retained
a failed partial report with zero samples and wrote the original Error to both
the default-level file and stderr. Info source-plan start was present and CLI
stdout was empty. All 64 original prepared input hashes remained unchanged.
`data/depth75-reference-probe-native-v1.json` identifies the verified
17-member archive with both original outcomes and their source boundary.

No model predictions have been generated by the probe yet. RGB decoding remains
an earlier separately identified stage; its cost must not be hidden inside,
or claimed as part of, worker inference timing. Actual cohort measurements,
independent held-out reduction and the other remaining acceptance items are
still pending.
