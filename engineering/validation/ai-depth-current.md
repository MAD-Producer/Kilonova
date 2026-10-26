# Optional desktop depth inference — issue #74

This branch implements #74 after the typed render DAG (#78). Android is retired
and is outside implementation and acceptance. A contract checkpoint is not a
completed inference feature. No existing project migration or UI change is added.

## Resource contract

`motion_model::depth` owns an immutable, validated R32Float scalar resource with
separate optional validity and confidence planes. It carries a bounded model
identity, exact weight hash and processing identity, source asset/revision and
integer presentation timestamp, crop/clockwise rotation/mirroring, decoded
scale/offset and optional valid decoded range. The output has explicit pixel and
byte budgets. No inference library or GPU API enters the model dependency closure.

Relative depth carries its near direction and remains unitless, even for a 0..1
prediction. It cannot be used as camera Z or a world distance without a separately
defined calibration. Metric and inverse-metric values carry units and distinguish
camera Z from distance along a ray; converting ray distance requires the actual
camera-ray cosine. These values still require the renderer's projection before
they can be used as hardware depth. Declared sample bounds are not clip planes.

UV mapping operates on continuous image edges; texel centres use `(i + .5) / size`.
Pixels outside the source crop are ordinary misses, not clamped depth samples.
An invalid pixel may retain NaN, but every valid decoded sample must be finite,
within declared bounds, and physically positive when it has metric semantics.
Absent confidence is unknown, not an invented score of one.

Initial source-frozen acceptance: `issue74-contract-v1` ran seven new contract
tests. After exposing identity validation for the optional host, the complete
`cargo test -p motion-model --locked --offline -- --test-threads=1` regression
(`issue74-contract-v2`) passed 81 tests across 21 targets, including those seven,
with zero failures or ignored tests. Input hashes were unchanged through the run.
The command, outputs and hashes are retained in the contract receipt under
`engineering/validation/data`; this does not cover a future inference backend.

## Remaining acceptance

- Implement the optional desktop ONNX host, explicit preprocessing and output
  semantics, CPU fallback and actual provider/copy-cost reporting
- Verify asynchronous cancellation and source/revision/PTS/model ownership before
  installation, including controlled blocking, seek and replacement
- Keep model download/import, revisions, hashes and licence records separate from
  project/effect packages; no weights are bundled in an `.msfx`
- Compare a real, verifiable model against its official reference numerically and
  at edges; cover portrait, illustration, interior and a video sequence, with
  limitations for out-of-domain quality and temporal stability
- Complete actual failure/default-output checks and maintained desktop regression

Official references read for the prototype:
[Depth Anything V2](https://github.com/DepthAnything/Depth-Anything-V2),
[Small model card](https://huggingface.co/depth-anything/Depth-Anything-V2-Small),
[ONNX Runtime Python API](https://onnxruntime.ai/docs/api/python/api_summary.html),
[execution providers](https://onnxruntime.ai/docs/execution-providers/).
No inference capability, zero-copy path or model quality claim is made by this
contract checkpoint.
