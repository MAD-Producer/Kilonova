# Pinned indoor metric candidate (#75)

Status: source and weights acquired and verified; export admission implemented.
Actual numerical export, sensor/domain accuracy and inference admission remain
unverified. Desktop-only scope; no model enters default distribution.

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
These steps are implemented but have **not executed for this checkpoint**.
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
followed this native failure.

Private original controllers, logs, license, ZIP inventory and receipts reside
under `artifacts/pr-queue/depth75-metric-acquisition-v1` and
`depth75-metric-environment-resume-v1`. No weights, upstream implementation,
runtime or learned tensors are included in this document or repository.

The next prerequisite is a verified export environment and actual same-weight
CPU numerical comparison. Only then may the distinct metric processing route
enter host inference, followed by the existing independent held-out sensor
evaluation, resource inventory, WGSL equivalence, cross-resolution and thermal
gates. The original relative-model domain/transition failures and complete native
regression failures remain recorded in `depth75-status-current.md`; these new
admission controls do not clear them or complete #75.
