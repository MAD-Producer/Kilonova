# Metric native-aspect operator and resource inventory (#75)

Status: static inventory complete for the separately identified indoor metric
candidate. The original sensor-quality gates remain failed. No learned inference,
new device, model acquisition, backend admission or default distribution is
performed by this inspection.

The inspection binds the exact ONNX SHA-256
`4d7bbff0e1f3cf79e79df327f8624916dc55f52ac624b53dc6336b0a9f05842b`
from the [actual official-aspect export](depth75-metric-native-aspect-current.md).
The model remains outside the repository. Its checkpoint, upstream revision,
CPU environment and Apache-2.0 Small provenance are unchanged.

## Observed graph

| Item | Observed |
| --- | --- |
| Standard ONNX opset | 17 |
| Nodes / operator kinds | 607 / 26 |
| Input | Float32 RGB `[1,3,518,686]` |
| Output | Float32 camera-Z metres `[1,518,686]` |
| Stored initializers | 234 Float32 tensors, 24,710,465 elements, 98,841,860 bytes |
| Unknown tensor sizes after bounded shape resolution | 0 |
| Quantization operators | None |
| Persistent initializer/Constant storage | 98,843,400 bytes |
| Naive materialized activation peak | 171,880,128 bytes, Softmax node 414 |
| Static persistent + activation amount | 270,723,528 bytes |
| Largest nonconstant tensor | Float32 `[1,6,1814,1814]`, 78,974,304 bytes |

Operator counts and their exact attribute variants are retained in the receipt;
all node output shapes, element types and byte sizes are retained in the tensor
inventory. This graph includes Sigmoid, Cast and Identity; it must not reuse the
old relative model's 598-node inventory, shape identity or resource estimates.
Learned data remain Float32; integer and boolean shape-control values are also
inventoried. This is not an int8 or Float16 candidate.

ONNX shape/data propagation leaves two symbolic tensors. An input-free seven-node
integer program resolves Expand `[1,1,384]` and Concat `[1,1814,384]`; the latter
is cross-checked against the independently inferred downstream Add. No image,
learned operator or model weights are evaluated by that shape program.

## Estimate boundaries

The serial allocation simulation materializes outputs before releasing last-use
inputs. A separate inclusive producer/last-use interval enumeration reproduces
both the peak bytes and its node index. This cross-check is about that explicit
static schedule. It does not establish a physical device or process memory peak.

Aliasing, fusion, arena alignment, allocator reuse, upload/readback staging,
driver/pipeline allocations, loader copies and other editor resources are not
modeled. The largest individual tensor is below the previously documented
128 MiB storage-binding comparison baseline; actual adapter capabilities,
workgroup sizes and storage usage have not been queried or qualified here.
The combined static amount spans multiple buffers and must not be compared
to a single-buffer limit as if it were one allocation.

The existing relative WGSL host plan does not admit this metric processing
identity or non-square shape. This inventory supplies no Sigmoid/head execution,
same-weight WGSL equivalence, latency, thermal or full-path memory qualification.
The actual metric sensor RMS and maximum remain above their unchanged bounds;
the candidate is not promoted to an automatic quality tier.

## Retained evidence

The [archive](data/depth75-metric-native-static-v1.zip) and
[manifest](data/depth75-metric-native-static-v1.json) retain the independently
authored controller, receipt, phase log and tensor metadata. CRC and every
member's byte-for-byte readback passed. No ONNX graph, weights, learned tensors,
upstream implementation, source photographs or sensor payloads are included.
Original terminal `9cc1b6` completed with exit0; no running handle remains.
Product source stayed at `757af4e6`; no product code or assertion changed.

This completes this candidate's static inventory, not requirement I1's domain
qualification, I2's dual-backend execution, or the full #75 acceptance.
