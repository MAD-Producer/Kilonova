# Official-aspect indoor metric research profile (#75)

Status: numerical export and the fixed32-input measurement complete; the
candidate still fails the original sensor-error gates. It remains external,
explicit and unregistered as a default host inference/quality route.

## Independently justified processing change

The pinned official metric `image2tensor` uses aspect-preserving lower-bound
cubic resizing with dimensions constrained to multiples of14. It scales RGB
values to Float64 before resizing. For640x480 this gives686x518; it does not
produce the previous518-square black letterbox. The official training dataset
also converts radial Hypersim distances to camera-Z metres before training.
These facts were inspected in the already pinned unmodified reference, not
inferred from candidate errors:

- [Pinned preprocessing](https://github.com/DepthAnything/Depth-Anything-V2/blob/a561b849ebae10a6f5ef49e26c83cbbcd36c71bf/metric_depth/depth_anything_v2/dpt.py)
- [Pinned resize](https://github.com/DepthAnything/Depth-Anything-V2/blob/a561b849ebae10a6f5ef49e26c83cbbcd36c71bf/metric_depth/depth_anything_v2/util/transform.py)
- [Pinned Hypersim conversion](https://github.com/DepthAnything/Depth-Anything-V2/blob/a561b849ebae10a6f5ef49e26c83cbbcd36c71bf/metric_depth/dataset/hypersim.py)

The new explicit `--metric-hypersim --metric-native-aspect` export keeps those
same verified weights, reference and CPU environment. Relative and square
metric identities/behavior remain separate. Unsupported flag combinations
refuse before IO, including optimized Python. The profile has a distinct
processing identity, filename and input/output shapes, and declares its640x480
source extent. It is not an implicitly variable-shape model or a new quality tier.
No new model, runtime, dependency or media was acquired.

The independent `preprocess_metric_native` enforces RGB8 640x480, resizes the
normalized floating RGB plane to686x518, and produces contiguous Float32 NCHW.
During measurement it matched the unmodified official Resize/Normalize/Prepare
transform composition exactly on all32 original inputs. Output restores source
coordinates with the official Torch bilinear align-corners operation; the owned
row-bounded remap was independently checked against it on each output under
the unchanged2e-4 maximum /2e-5 mean numerical bounds.

## Actual numerical export and sensor result

The actual CPU reference/ONNX comparison passed the unchanged numerical bounds.
Model hash and exact measurements follow in the table. The32 original RGB and
sensor hashes, ordering, first8 diagnostic/next24 primary split and all nonzero
sensor samples remain unchanged. Camera-Z metres are compared directly through
reciprocal prediction versus5000/raw; there is no affine fit, depth clipping,
per-frame normalization, residual trimming or annotation-based input selection.

| Measurement | Observed | Original bound |
| --- | --- | --- |
| ONNX reference maximum normalized |3.0790541497543856e-06|2e-4|
| ONNX reference mean normalized |7.568779500429062e-07|2e-5|
| Primary sensor RMS inverse metres |0.16983065169410547|0.01|
| Primary sensor maximum inverse metres |1.305050949890166|0.05|

The primary24 frames retain5,637,354 valid and1,735,446 missing sensor pixels.
The smaller RMS compared with the previous square candidate does not grant
qualification: **both original sensor bounds still fail**. The square-profile
and older relative-model failures remain failed. Numerical preprocessing/export
agreement is not sensor accuracy, WGSL equivalence, memory/thermal or video
transition qualification. No automatic backend/model/tier selection changes.

Export session77031 and sensor session14341 completed with exit0, original
terminals`d077db` and`7b3c1d` consumed. The sensor controller's exit0 means its
measurement finished; its explicit quality verdict is rejected. Relevant source
hashes remained frozen through each command. The later preprocessing helper was
added after numerical export; the sensor receipt binds the helper's exact hash.
The archived final source must not be misidentified as the earlier export hash.
Seven metric admission controls pass, including two new real profile/refusal
controls. No existing assertion or threshold changed.

## Complete Python CI preflight and original failures

After the inventory correction at6c6d8c7a, complete CI Python discovery first
reported127 tests, two module-import errors and one existing skip. Its selected
standard-library interpreter lacked yaml; main CI explicitly installs
PyYAML6.0.3. Original logs remain failed (session81788, terminal`a26ee9`).

A separate continuation used the existing E-drive environment whose installed
metadata reports that exact PyYAML6.0.3, without installation or mutation of its
packages. Complete discovery then ran155 tests:154 passes and one original skip
(session37107, terminal`ae46cd`). This covers the earlier inventory fix source;
it precedes this native-aspect increment and is not its final regression. The
original intermittent WindowsPath fixture error was not diagnosed or repaired
by these later passes. No debugger, interpreter/system modification, native
compiler/backend change or unchanged rerun was used to clear that failure.

## Retention and remaining acceptance

The [archive](data/depth75-metric-official-aspect-v1.zip) and
[manifest](data/depth75-metric-official-aspect-v1.json) retain authored controllers,
source, original receipts, logs and the model's metadata/hash. Every member passed
CRC and byte-for-byte readback. No learned tensors, ONNX graph, source images,
sensor payloads or upstream implementation are redistributed. These original
inputs remain under their previously recorded external sources/licenses.

The external ONNX artifact is98940404 bytes, SHA-256
`4d7bbff0e1f3cf79e79df327f8624916dc55f52ac624b53dc6336b0a9f05842b`.

Actual domain quality, independent WGSL/resource qualification for this shape,
qualified cross-resolution/video transitions, conservative complete memory and
thermal qualification remain pending. #75 and the full3D route remain incomplete.
