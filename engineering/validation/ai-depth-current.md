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

The optional `motion-host/ai-depth` job layer now retains one running request
and one replaceable pending RGB8 input. It publishes in-flight state and Info
start before entering its backend. Tickets, exact rational source PTS, asset
revision, geometry and full model identity are checked before output can cross
the installation boundary. Cancellation and superseded inputs remain normal
discards. An observable backend panic stops that backend; it is never restarted.
Callers must explicitly cancel on a seek or supply the new live key when taking
output. This API does not yet integrate depth into Session or the desktop UI.

Six source-frozen tests (`issue74-jobs-v1`) passed with no failures or ignores:
controlled blocking proves flight publication before backend entry; 97 queued
replacements retain only the newest; late canceled successes cannot install;
seven source/model/geometry mutations are rejected at installation; equivalent
rational timestamps remain valid; missing models, wrong backend identities,
unknown providers, input budgets and terminal backend panics preserve errors
and permit recovery where appropriate. Default-Info start/Error records are
checked through the real log facade. This capture does not itself prove the
desktop file/stderr sink, which retains its existing acceptance coverage.
Counts saturate over the lifetime of a Jobs instance: completion/failure/cancel
counts describe backend exits; pending replacements have their own count;
stale counts describe completed results discarded at the installation boundary.
There is no implication that occurrences correspond to wall-clock seconds.

Evidence: [job source, commands, output and hashes](data/ai-depth-jobs-fb35fe19.zip),
SHA-256 `42549b2d3cbfa67542c85dd554442ecd88221356f01928afff797341476374c5`;
252,927 bytes, eleven payload files, read back and verified after archiving.

Separately, the official Apache-2.0 Small reference was exercised outside the
product tree, using verified weights (SHA-256
`715fade13be8f229f8a70cc02066f656f2423a59effd0579197bbf57860e1378`)
and reference revision `a561b849ebae10a6f5ef49e26c83cbbcd36c71bf`.
A fixed 518-square ONNX export ran on the actual CPU execution provider; maximum
normalized error against the official PyTorch forward was approximately
3.1844e-6 on one authored fixture. These external feasibility artifacts do not
establish arbitrary aspect ratios, model quality, asynchronous Session wiring,
or a production ONNX backend. Weights and reference code are not bundled here.

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
