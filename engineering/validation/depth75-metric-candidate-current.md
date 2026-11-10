# Pinned indoor metric candidate (#75)

Status: actual CPU numerical export passed; the fixed sensor-depth evaluation
rejected this candidate under the original bounds. Host inference admission,
WGSL equivalence and broader domain qualification remain unverified. Desktop-only scope; no model enters default distribution.

## Authorized candidate and provenance

The user explicitly authorized this candidate and an isolated CPU export
environment on E. The official Hypersim Small checkpoint is pinned to model
revision `3bc65d4e14a6786a61acec16453c50e12bf5f338`,99,222,290 bytes,
SHA-256 `b782898d8a3e8be1f639de33837ed85e9b4b73e40f8f5e5cd99067588d722545`.
The downloaded bytes satisfy both values. The official unmodified reference
checkout is `a561b849ebae10a6f5ef49e26c83cbbcd36c71bf`; its Apache-2.0 license
is retained. Authors, exact sources and modification scope are recorded in
`ATTRIBUTION.md`. All external files remain under `.tools/depth75-metric-export-v1`.

Read-only ZIP inspection found241 records with99,169,022 uncompressed bytes.
This is container metadata, not a tensor-shape, operator or numerical inventory.
No pickle contents were executed by that inspection.

## Export implementation and intended gate

`tools/export_ai_depth.py --metric-hypersim` selects the distinct official metric
reference module, checkpoint,20-metre head, model ID, output name and camera-Z
metre processing identity. This is initially a518-square candidate; other metric
resolutions refuse before IO or runtime imports. Existing relative266/392/518
profiles keep their identities and behavior. The metric path cannot accept the
relative checkpoint's byte size/hash or publish it as a metric candidate.

The exporter retains the existing normalized CPU-reference comparison bounds:
maximum2e-4 and mean2e-5. It checks both the reference and ONNX output for finite
values and the declared zero-to20-metre range. Publication remains after the
checker and actual CPU comparison, in a new directory with license/provenance.
The actual pinned-checkpoint export subsequently executed and passed; see below.
Default CLI file logs flush phase starts before blocking operations; errors also
reach stderr and stdout is not used for authored diagnostics. Strict admission,
no-overwrite and actual failed-CLI file/stderr/stdout controls accompany the code.

## Original bootstrap failures and current boundary

`depth75-metric-acquisition-v1` verified weights and fetched the pinned reference,
then reached the CPU PyTorch installation. Its original tool handle later became
unknown. The receipt still said running, but the recorded installer PID and the
exact owned controller command were absent; Torch/torchvision were not installed.
No terminal success or exit reason is invented for this interrupted operation.

The explicit continuation `depth75-metric-environment-resume-v1` reverified the
retained inputs and resumed installation in that same dedicated environment.
Its pip child exited3221225477 (`0xc0000005`) during installation, before any
model import, export or inference. Original session88296 exited1 and terminal
chunk`7dbc69` was consumed. Original logs and command receipts remain. No native
fault location, model defect, driver/overlay cause or successful environment is
inferred. No unchanged installation retry or environment/backend workaround
followed this native failure. A separate verified-wheel/offline installation
subsequently completed; it does not explain or resolve that original crash.

Private original controllers, logs, license, ZIP inventory and receipts reside
under `artifacts/pr-queue/depth75-metric-acquisition-v1` and
`depth75-metric-environment-resume-v1`. No weights, upstream implementation,
runtime or learned tensors are included in this document or repository.

## Verified environment and original acquisition failures

The changed bootstrap separately acquired and SHA-256-verified23 official
wheels, then installed locally with `--no-index`. The initial local install
refused because the wheelhouse lacked `setuptools>=77.0.3`; the metadata audit
also identified Windows-only `pyreadline3`. Both were acquired and verified
before the subsequent installation. The first extra-wheel metadata reader
incorrectly counted setuptools' vendored METADATA as top-level distributions;
the corrected reader restricts entries to the wheel root. Original failures
remain in their receipts; no incomplete wheelhouse is treated as successful.

`depth75-metric-local-install-v2` completed installation, `pip check`, actual
CPU imports and freeze. Installed primary versions: torch2.14.1+cpu,
torchvision0.29.1+cpu, ONNX1.23.2, ONNX Runtime1.31.0, NumPy2.4.6 and
opencv-python-headless5.0.0.93. CUDA reports null. No runtime enters distribution.
Official distribution URLs, hashes and metadata are retained in
`depth75-metric-wheel-stage-v1` and `depth75-metric-wheel-stage-dependencies-v2`.

## Actual export, interruption and diagnostic increment

The original export tool call returned aborted without a terminal exit result.
The exact owned controller/export command lines were absent on inspection.
Its960-byte receipt, controller, phase log and98,941,164-byte temporary ONNX
contained zero bytes throughout the inspected receipt/controller or sampled
model boundaries. No final output directory existed. Stderr contained reference
availability notices and the legacy ONNX warning, not a numerical verdict.
`depth75-metric-export-interruption-v1` preserves byte counts/hashes. Root cause
is unknown; this attempt is neither success nor an established model defect.

The changed exporter splits reference execution, ONNX export/check/session/run
into pre-operation phases with elapsed time. Metric CLI phase/error records now
flush and fsync; errors retain phase, elapsed time and their original cause.
The subsequent instrumented run, `depth75-metric-actual-export-v2`, completed
with exit0 and its original terminal consumed (session59380, chunk`da4efc`).
No model, runtime, upstream code, preprocessing or numerical bound changed.
The actual98,941,164-byte ONNX SHA-256 is
`b92146e04280b7b8c5ad859dfd40a944f1f4bbef8a7856af87e8f6d1ecfeb85d`.
Its reference probe maximum normalized error is6.745167797290904e-6 and mean
2.964247137302372e-6, below the unchanged2e-4 /2e-5 bounds. The default Info
file records every phase before entry and completion at12,234ms. Captured
stdout is empty; third-party notices remain in stderr. This diagnostic success
does not erase the interrupted attempt or establish why its files were zeroed.

## Fixed sensor-domain result: rejected

`depth75-metric-sensor518-v1` completed all32 original timestamp-selected TUM
freiburg1_desk pairs in original order. All RGB and UInt16 sensor input hashes
and the model hash were checked before inference. The first8 remain diagnostic;
the next24 remain the primary evaluation set. Unlike relative calibration,
declared camera-Z metres are compared directly: reciprocal prediction versus
raw sensor5000/raw. There is no affine fit, per-frame normalization, clipping,
residual trimming, input substitution or changed preprocessing. Every valid
nonzero sensor sample contributes; invalid model values would fail the run.

| Primary metric | Observed | Unchanged maximum |
| --- | --- | --- |
| RMS, normalized by exactly one inverse metre |0.27860860782373686|0.01|
| Maximum, same normalization |1.364245317991038|0.05|

The24 frames contain5,637,354 valid and1,735,446 missing sensor pixels. Sensor
holes are reported separately and do not permit hiding invalid predictions.
The evaluation controller finished exit0 (session73837, chunk`51ee06`), meaning
the measurement completed; its explicit quality verdict is **rejected**, not
qualified. Full raw Float32 metre planes, per-frame metrics, input/source/model
identities and pre-run bounds remain in that artifact directory. Source hashes
were unchanged across the measurement. This is one indoor sensor sequence,
not exact geometric ground truth or a certificate for other source domains.

The initial instrumented run passed4 metric and12 relative export-admission
tests (chunk`f3ad0a`). After adding secondary persistence-failure handling, the
final run passed all5 metric tests, but the relative suite reported11 passes
and one error (chunk`cfca5f`). Existing assertions are retained. The error is
`TypeError: unsupported operand type(s) for /: WindowsPath and str` inside the
existing bounded-site-enumeration fixture generator, before its expected
attestation refusal. The source of that unexpected Python failure is unresolved;
the later command is not a green regression, and no unchanged rerun followed. The added real-process
control injects a secondary fsync failure and verifies the original export
exception and stderr/stdout boundary survive. These controls do not provide
learned-model accuracy. The rejected candidate remains external and is not
registered as an automatic host inference route or product quality tier.
Actual metric WGSL/resource inventory, cross-resolution/video, conservative
memory and thermal qualification remain open. Original relative failures and
full native regression failures in `depth75-status-current.md` remain failures.


## Subsequent original-failure observation and CI inventory correction

One separate instrumented fixture observation wraps `PurePath._make_child` only
to record the underlying TypeError if it occurs. It does not replace results or
change the test assertions. It completed without reproducing the failure and
recorded no path-error events (`depth75-path-fixture-cause-observation-v1`,
exec`e7e65c`). This is not a repair and does not clear the earlier failed full
relative suite. No interpreter, backend or environment workaround was applied.

Exact published source9784a9f1 reached CI run42734644701. Its completed Rust/GPU
job129560767905 failed the tool inventory: the newly added
`tools/test_ci_metric_depth_export.py` was not classified. The original155-test
CI invocation reported one error and one skip. The job log is retained as
`depth75-ci-9784a9f1-rust-gpu.log`. This is an authored integration omission;
no Rust or model defect is inferred from that inventory exception.

The follow-up registers the new file as ordinary CI discovery, with its actual
admission/diagnostic scope. `test_ci_tools.py` subsequently ran20 tests with
one original skip and no errors (19 passes), exec`b0e81b`; original logs remain
in `depth75-ci-9784a9f1-inventory-fix-v1`. The existing assertions and inventory
gate are unchanged. This narrow correction does not replace a new complete CI
run, repair the earlier local fixture error, or qualify the metric model.


The separately identified [official-aspect profile](depth75-metric-native-aspect-current.md)
now exports the same weights at686x518 for640x480 source input, following the
pinned reference's aspect-preserving preprocessing. All32 preprocessing inputs
match the official transforms exactly. Primary RMS0.16983065 and maximum1.30505095
still fail the original0.01/0.05 sensor gates; neither profile is qualified.
