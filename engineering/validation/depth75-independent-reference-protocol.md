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

The first actual CPU266 cohort at `53d3f9ba` failed in the caller after its
first worker inference returned. The caller incorrectly compared the raw RGB
file SHA-256 to `PreparedDepthInput.rgb_sha256`, which includes the versioned
RGB contract and dimensions. No sample or prediction array was accepted and
no sensor metric was computed. Original session 22317 was consumed with exit 1;
the source was unchanged. `data/depth75-reference-onnx266-failure-v1.json`
identifies the verified archive containing the original error, default-level
Info start, Error file/stderr output and empty stdout. It remains a failed run.

The caller now checks the raw file digest before submission and independently
constructs the versioned prepared identity for the returned input. It compares
the whole identity, including dimensions. A new control uses an actual `Jobs`
worker with a controlled backend to check the fingerprint contract and reject
raw-digest substitution, transposed dimensions and one-byte replacement. The
original source, role, request, provider, timing and output-extent checks remain.
Mismatch errors now retain bounded actual/expected fields. This correction
changes the evaluation caller, not the production inference or cache contract.
Its native tests, rebuilt CLI and next actual cohort are recorded separately below.

RGB decoding remains an earlier separately identified stage; its cost must not
be hidden inside, or claimed as part of, worker inference timing.

## First completed actual reference cohort

At `08dddbb9`, all three native caller controls passed without ignored/filtered
tests. The rebuilt CLI also retained the original pre-inference source-refusal,
default Info file records, Error file/stderr output and empty stdout. Original
native session 86270 and actual cohort session 76813 were consumed with exit 0.
Both kept exactly the same frozen source. Evidence is identified by
`data/depth75-reference-onnx266-v2.json`; its archive includes all raw measurement
rows and logs but no model, RGB/sensor images or learned prediction arrays.

All 32 CPU266 requests completed without failure, cancellation, cache reuse or
restoration. All private prediction arrays were independently read back and
SHA-256 checked before archiving. The original eight calibration frames used
2,020,231 valid pixels and retained 437,369 missing pixels. Their one fitted
inverse-depth map has slope 0.20058113727943536 and offset 0.3929613881077111.
The following 24 held-out frames retain 5,637,354 valid and 1,735,446 missing
pixels. No frame was replaced, refitted, clipped or selected by residual.

| CPU266 held-out statistic | Inverse metres |
| --- | ---: |
| Pixel-weighted RMS | 0.13330743961888267 |
| Mean absolute error | 0.11466807898351689 |
| Maximum absolute error | 1.0977689458419422 |

Normalization is exactly one inverse metre. These measured residuals are not
ONNX/WGSL numerical-equivalence errors or a quality-tier admission. All 32 cold
worker observations are retained: fastest 2.4321458 s, median 2.8730603 s and
slowest 6.0936272 s. They include source fingerprinting, execution resolution,
inference and transport, while excluding caller PNG decoding and queue wait.
The result does not demonstrate real-time interactive inference.

The next five routes remain in the original declared order: WGSL266, CPU392,
WGSL392, CPU518, WGSL518. Inspection establishes that the current pinned WGSL
implementation accepts only its 518 model/program/processing identity. Its 266
and 392 routes must therefore retain explicit unavailable results, without a
different backend, model or program being substituted. They cannot count as
successful model measurements. All available routes retain the same 32 frames
and eight/24 division. Missing routes do not authorize changing the protocol.

Additional transition jump remains unknown. Arbitrary-domain quality, complete
memory/weight/activation peaks, longer thermal behavior, quality-tier selection
and the remaining issue acceptance are still pending.

## Completed declared route inventory

At `e7461aca`, original session 42461 completed and was consumed with exit 0.
The complete source snapshot remained unchanged. The original declared route
order was retained. CPU392, CPU518 and WGSL518 each completed all 32 requests;
WGSL266 and WGSL392 each explicitly refused their unsupported model/processing
identity before native inference. Both refusals preserved the original cause
in default-level file/stderr output, empty stdout, zero accepted samples and
no substitute backend or model. An unavailable route is not an inference pass.

| Route | Held-out RMS (m^-1) | Maximum (m^-1) | Median worker seconds | Worst worker seconds |
| --- | ---: | ---: | ---: | ---: |
| CPU266, preceding cohort | 0.1333074396 | 1.0977689458 | 2.8730603 | 6.0936272 |
| CPU392 | 0.1222982912 | 1.2110056933 | 3.8076713 | 8.7054381 |
| CPU518 | 0.1099756634 | 1.1606672852 | 4.2956094 | 8.4155761 |
| WGSL518 | 0.1099756357 | 1.1606679537 | 16.6736628 | 21.3163454 |
| WGSL266 / WGSL392 | unavailable | unavailable | unavailable | unavailable |

Every available route uses the same eight calibration and 24 held-out frames,
with identical valid/missing coverage. Each retains all cold requests, including
the slowest observation; no warm-up observation was discarded. No observed
route establishes real-time performance. Larger resolution reduced RMS on this
cohort but did not monotonically reduce maximum error; this is not a basis for
assigning a tier without independently declared acceptance limits.

After the original controller was consumed, all 32 paired CPU518/WGSL518 outputs
were compared independently, including pixels missing from sensor ground truth.
The original same-weight numerical bounds remain maximum 0.0002 and mean
0.00002, normalized by each CPU output span. All frames passed: largest observed
maximum 0.000027715290698834655 and largest frame mean
0.0000003761387523919777. This numerical-equivalence normalization is distinct
from the fixed physical normalization of the independent sensor metric.

`data/depth75-reference-remaining-v1.json` identifies the verified 30-member
archive. All 96 private prediction arrays from this remaining controller were
independently read back and SHA-256 checked before archiving. The archive retains
every measurement/quality row, explicit refusal, original logs, controllers and
identities, without redistributing model, sensor/RGB or prediction arrays.

## CI discovery follow-up

The two analytic test modules now use the existing `test_ci_*.py` discovery
pattern, and all four helper/test paths are explicitly classified in
`tools/validation_inventory.json`. No new CI job or optional runtime is needed.
The 13 original analytic checks use explicit `AssertionError` failures so that
Python optimization cannot remove them. An AST comparison verified every
original condition and all four numerical functions unchanged. A new child
control exercises the actual failure under normal and optimized Python.

The proposal passed all 11 controls and the complete inventory check in an
isolated source copy while inference source remained frozen. That preflight
is not a final-source CI pass. Actual-source discovery/full tools validation
and current-head cross-platform acceptance must be recorded separately.
