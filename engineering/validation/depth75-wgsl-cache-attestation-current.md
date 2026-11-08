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
- The shared helper is also verified with the actual separately installed Small
  266 CPU ONNX model. The original file-cache probe assertions are unchanged;
  only the harness source paths point to this branch. One inference populated
  the cache, a new worker restored all 96 scalar bit patterns, and a corrupt
  entry was refused without another inference. Default Info publication before
  the controlled handler returned, Error file/stderr and single-JSON stdout
  isolation passed. This synthetic input does not qualify model/video quality,
  sustained performance or native WGSL caching. Receipt:
  `artifacts/pr-queue/depth75-wgsl-shared-runtime-cpu-model-v1`

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

The native worker now has separate `--resolve` and `--attest` entry points.
Resolution requests have a strict input-free schema; inference requests retain
their existing input tensor contract. Before publishing a native identity, the
source verifies every admitted pinned weight resource and includes its own
executable digest, the actual adapter/driver metadata, required device limits,
explicit Float32 procedure and bounded native numerical environment. The
attested inference path compares that identity before learned operations.
Missing driver identity, software adapters and changed execution are refused;
the ordinary inference invocation remains available independently.

Current product worker sources compiled in a minimal isolated harness using
only verified lockfile dependencies. Its 16 admission tests pass, including
worker/driver/backend/limit/program identity and bounds controls. A real default
Info child invocation retained the original invalid-request failure in both
the file log and Error stderr, with an empty stdout and zero encoded model
operations. These checks do not exercise successful hardware resolution or
actual attested model inference. The harness is not a product-package build.
Receipt: `artifacts/pr-queue/depth75-wgsl-native-identity-contract-v2`. The v2
source retains the ordinary invocation's original version-before-shape error
precedence; the earlier v1 receipt remains a separate historical stage.

The host must next invoke and independently validate native resolution before
lookup, combine it with the transform identity, and request/recheck attested
preparation, native inference and remapping on a miss. Cancellation, cleanup,
original causes, in-flight
publication, file/Error-stderr logging and stdout isolation must be retained.

Acceptance still requires a real native WGSL inference, a fresh-worker raw-cache
restore without inference, invalidation/refusal controls, current alignment
admission, full regression and exact-head cross-platform CI. Broader #75 video
quality, backend comparison, full-path latency, memory and sustained-load
qualification remain separate obligations. No panels, UI, new dependency or
third-party asset is changed here.
