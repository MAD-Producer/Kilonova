# Issue 75: WGSL persistent-cache execution attestation

Status: in progress. This stacked branch starts at cache PR #649 head
`13675ddeabb4979e7f6b76c078adc38ec56db054`. Neither #75 nor #88 is complete.
The maintained delivery scope is desktop; Android is not being qualified.

## Transform execution identity

The native WGSL inference path uses Python NumPy/OpenCV for RGB preparation
and depth remapping. These two phases must be part of the cache execution
identity even though they do not execute the learned model. They do not need
ONNX Runtime installed.

`tools/depth_tensor.py --resolve` reports the actual numerical runtime and the
two embedded procedure sources without reading RGB/depth inputs or invoking
learned operations. `--attest <directory>` requires an explicit
`transform-execution.json` and compares that identity before reading the input
or replacing an output. It returns its observed execution identity alongside
the transform receipt. The original one-directory invocation remains unchanged.

The runtime digest includes the selected packages' Python/native code, relevant
sibling numerical libraries, CPU dispatch features, Python/OS/build information
and bounded numerical environment. Package sites may differ; dependency
enumeration order must not change the identity. Enumeration, source bytes,
numerical context and expected-identity metadata are bounded. Raw environment
values are hashed and are not emitted in receipts or logs.

CPU ONNX now uses the same bounded numerical-runtime helper with its own domain
and ORT build identity. Its hash domain advances to v2; previously computed
identities conservatively miss rather than being treated as interchangeable.
No old-project migration or compatibility fallback is introduced.

## Evidence and limits

- The existing `test_ci_depth_tensor.py` and `test_ci_depth_export.py` tests are
  preserved. New tests are in `test_ci_depth_tensor_identity.py`
- The Python depth controls pass: 24 tests, including real child-process
  protocol tests with an import guard that rejects ONNX Runtime imports
- The synthetic-package controls prove identity/admission behavior, not actual
  learned inference, GPU execution or model quality
- An actual installed NumPy/OpenCV transform comparison passed for both RGB
  preparation (3,219,888 bytes) and remapping (140 bytes), matching the previous
  procedure byte for byte. An altered expected identity was rejected without
  replacing an existing output. The v2 receipt repeats this on the current
  bounded-context implementation, with source hashes unchanged
- Scoped host/cache regression passes: five requested Rust targets, 269 passed,
  zero failed, 40 original GPU-dependent ignores and zero filtered. GPU features
  were not requested for this scoped invocation; these ignores are not GPU
  acceptance. No complete host/render/media regression pass is claimed

The actual numerical receipt is local under
`artifacts/pr-queue/depth75-wgsl-transform-validation-v2`; the scoped regression
uses `artifacts/pr-queue/depth75-wgsl-transform-contract-v1`. These are independent
evidence stages and do not replace the original failed full regression on
PR #649. That PR remains a draft despite green remote checks.

## Remaining execution boundary

The product host does not yet call the new transform identity modes. WGSL
persistent-cache requests still fail explicitly through the existing unsupported
execution-identity boundary; they must not claim that this transform precursor
alone makes a GPU result reusable.

The next implementation must resolve and validate the pinned native program,
all installed weight resources and the actual admitted adapter/driver/device
contract before lookup. The worker must recompute that identity in the actual
inference child before learned operations, and the host must combine it with
the transform identity. Cancellation, cleanup, original causes, in-flight
publication, file/Error-stderr logging and stdout isolation must be retained.

Acceptance still requires a real native WGSL inference, a fresh-worker raw-cache
restore without inference, invalidation/refusal controls, current alignment
admission, full regression and exact-head cross-platform CI. Broader #75 video
quality, backend comparison, full-path latency, memory and sustained-load
qualification remain separate obligations. No panels, UI, new dependency or
third-party asset is changed here.
