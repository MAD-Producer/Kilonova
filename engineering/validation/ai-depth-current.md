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

## Real optional ONNX host checkpoint

The optional host now embeds an independently authored file-protocol worker.
Python, its packages, the upstream reference and model weights remain external;
there is no network use during inference and no model in an effect/project
package. `tools/export_ai_depth.py` verifies the pinned reference and Small weight
hash, checks an actual CPU export against the official PyTorch forward, and
publishes the ONNX artifact with the original license and a separate manifest.
The tool reproduced the accepted 98,941,070-byte ONNX export, SHA-256
`274368034c0fcef08f9b197d33b606bae9c578acca861ef72feb268143b21434`.
The original Small-weight hash and exported ONNX hash are different identities.

The host uses bounded private stdout and stderr pipes, explicit scratch roots,
cooperative cancellation with child kill/reap, and primary-error preservation
when cleanup also fails. It records actual profiled providers, not the requested
provider alone. Costs include full backend wall time, parent staging and output
ownership, child tensor copies and inference phases; startup, model loading and
temporary cleanup are real costs. There is no persistent runtime, I/O Binding,
GPU interoperability or zero-copy claim.

`issue74-real-rust-v3` exercised the actual Rust Jobs/Onnx path, real model and
the existing desktop file logger at its default Info level. All four cases pass:
CPU inference, unavailable CUDA falling back to profiled CPU, missing model,
and bad model hash followed by successful recovery on the same Jobs instance.
In-flight request/source state is observed during real inference, returned
identity and R32Float values agree with the independent reference, and empty
queue/in-flight state and scratch cleanup are checked afterward. Failures retain
`request=1`, `asset=74`, `phase=model_hash` and the original cause in both file
and stderr; stdout is empty in every case. The external harness has no GUI
Engine; its panic command context is explicitly absent, not a GUI acceptance
claim. The maximum normalized numerical error is 2.14214e-6 against the unchanged
2e-4 maximum and 2e-5 mean bounds.

The model is hashed and loaded from the same bounded owned byte snapshot.
`issue74-model-snapshot-v1` replaces an owned copy precisely between verification
and runtime construction: the previous path-loading worker really fails in
ONNX Runtime with InvalidProtobuf, while the corrected worker still executes
the verified snapshot and agrees with the official reference. The original
model is hash-checked before and after; neither weights nor a model copy are
retained in the repository.

`issue74-domain-matrix-v2` passes fifteen real Rust/model/reference cases: NASA's
public-domain Eileen Collins portrait, an independently authored flat
illustration, an authored ray-traced interior, and twelve frames of authored
camera motion at exact rational PTS. Reference letterbox geometry is derived
independently with integer arithmetic; official normalization/forward and
PyTorch interpolation supply the comparison. Maximum normalized error is
2.41116e-5, within the original bound. Input/official/host comparison images and
the per-frame sources, timing, identities and hashes are retained. The synthetic
interior's inverse-camera-Z Pearson correlation is approximately 0.962–0.981;
this finite quality probe does not calibrate the unitless model output.

The domain evidence has explicit limits. The interior and camera sequence are
synthetic, not a natural interior photograph or live-action video. A candidate
NASA interior image was not acquired (the server returned 429); it is not counted
as tested. The model imposes apparent depth on the deliberately flat illustration
and does not guarantee fine-contour accuracy. Per-frame changes include true
parallax and are not a pure flicker metric. No accelerated provider, long video,
temporal model, Session installation or desktop UI feature has been accepted.
The original cancellation/seek/replacement installation tests remain necessary.

The first repeated optional-host compile was rejected: a shared target supplied
the other worktree's model artifact without the new depth module. After the
render run finished, only the affected workspace-package artifacts were
invalidated; third-party caches, sources and evidence were preserved. The next
four unit tests passed, but the controller incorrectly expected ten: the other
six tests are a separate integration target. That selection error is not a
product failure or an accepted ten-test run. No existing assertion was changed.

The corrected source-frozen `issue74-depth-regression-v3` runs both targets
explicitly. All four real-child ONNX tests and all six job integration tests
pass, with zero failures or ignores. They cover separate protocol/diagnostic
pipes, bounded overflow, original failure causes, actual child entry and
cancellation/reaping, as well as the existing blocked-worker, replacement,
seek/source/model identity and recovery checks. Changed host files also pass
their targeted rustfmt check; unrelated pre-existing host formatting is retained.

Those initial gates did not cover maintained-platform optional-feature CI or
natural-domain evaluation. The current Windows CPU prototype is not a completed
desktop feature. Later evidence below extends that scope. [Host setup and ownership
contract](../host-api/optional-depth-inference.md) describes how to use it.

Evidence: [runtime, reference comparisons, failure paths and source receipts](data/ai-depth-onnx-d84157be.zip),
SHA-256 `cfbd5b1f6e4482600a8dbce9ff28aa3b0b798b608326e715f1ee4b816e3fe9ef`;
7,036,560 bytes, 248 entries, every entry read back and SHA-256 verified.
The archive includes the original Apache license and model manifest, not model
weights, upstream implementation or runtime binaries. Rejected cache/selection
runs are retained separately from accepted receipts.

## Integration after the #78 budget correction

The three depth commits were rebased onto #78 head `8ffb5af7`, preserving only
their depth-specific changes. The rebased source-frozen gate
`issue74-depth-regression-v4` passes 91 tests with no failures or ignores: four
actual-child ONNX unit tests, six job integration tests, and the complete
81-test model regression. The previously archived real-model procedure itself
is unchanged by the rebase; this new run verifies integration with the current
main-derived model/render changes rather than replaying the old build result.

The existing native desktop CI test command now explicitly enables
`motion-host/ai-depth`, covering the optional contract/process tests on the
maintained desktop matrix without installing model weights or an inference
runtime. It adds no job, matrix entry or second full test command. Twelve
workflow-policy tests pass with the pinned PyYAML 6.0.3; the provenance guard
also passes. Remote results for this optional feature remain pending.

Evidence: [rebased Rust and workflow receipts](data/ai-depth-rebased-regression-609e793d.zip),
SHA-256 `a429e1616bedb07a6bb4047c28c339b2d650dafa6ac5070d1ac55cfe9dc74388`;
216,446 bytes, 17 entries, every entry read back and hashed. The workflow feature
selection was added after the frozen Rust gate and validated separately.

## Natural interior and real video evaluation

`issue74-natural-domain-v1` exercises seventeen additional actual Rust/ONNX
requests against the pinned official Small reference: NASA's 2003 Destiny
laboratory photograph, twelve consecutive frames of NASA's real ISS tour,
and four spaced anchors from other parts of the tour. The publicly supplied
15 fps variant has 13,453 decoded frames. FFprobe verifies every frame's actual
PTS against its `1/15000` time base; each request carries the original PTS, not
a guessed playback time. The twelve consecutive frames cover only 0.733 seconds.

The tagged limited-range BT.709 video is explicitly decoded and converted to
sRGB RGB24 with FFmpeg/zscale before entering the host contract. The untagged
still JPEG is interpreted as sRGB and resized to an odd 321-by-219 derivative
fixture. The source variant hashes, library revisions, conversion commands,
frame indices, exact timestamps and derived-input hashes are retained. Source
audio and original media are not redistributed. NASA attribution and its
factual/informational media-use terms are recorded in `ATTRIBUTION.md`.

All seventeen requests pass the unchanged normalized maximum `2e-4` and mean
`2e-5` numerical bounds, with actual CPU node execution, default-Info file logs,
empty stdout/stderr and the expected resource identities. The maximum observed
pixel error is 1.41701e-5; the maximum first-difference error is 1.39445e-5,
within the derivative bound implied by the existing pixel bound. A one-pixel
misregistration control fails the original bound on every case; its smallest
maximum error is 0.1392. These checks validate the host procedure and contour
registration against the reference, not the reference model's scene accuracy.

The NASA/input, official and host comparison sheet was visually inspected.
The model describes broad corridor depth but softens equipment and fine
boundaries. A descriptive optical-flow-compensated residual over the short
sequence has mean 0.0130–0.0352 and 95th percentile 0.0330–0.1052 after per-frame
reference-range normalization. This diagnostic includes camera motion,
occlusion, relative-scale changes and flow errors; it is not a pure flicker
score or a passed temporal-quality threshold. No metric ground truth,
universal domain coverage, calibrated hardware depth, accelerated execution,
long-video quality or Session/GUI installation is established by this run.

Evidence: [natural-domain sources, frames, numerical/edge controls and receipts](data/ai-depth-natural-domain-e90a6c8b.zip),
SHA-256 `57cd71df263d336f80069913789211146ebaa6cebef994452a8308d2be5e61f7`;
11,956,060 bytes, 204 entries, every entry read back and hashed. Original NASA
media, audio, weights, the upstream implementation and runtime binaries remain
external. The failed first extraction (an unsupported pixel-format negotiation)
is retained separately; it produced no accepted frames. Explicit planar RGB
negotiation fixed extraction without changing inference or numerical bounds.

## Main integration after the render-graph merge

The five prototype commits were replayed onto main
`eb2e8c8cc158beb37d539cc95d0a33b13f60332e`, which contains the final issue #78
render-graph implementation. The resulting executable-source head is
`bb4982564623ecc6ef4b4c51416512b061556456`; range-diff confirms that all five
patches are unchanged. This removes the dependency on the former feature-branch
base rather than merging the prototype into that branch.

Post-integration native validation passes 91 tests, zero failures and zero
ignored tests: four optional ONNX unit tests, six asynchronous depth-job
integration tests and all 81 model regressions. Every step records the source
hashes before and after execution and confirms they are unchanged. Model
weights and an inference runtime are not installed by this regression; the
earlier real numerical/reference evaluations remain separate evidence.

Evidence: [main integration regression and source receipts](data/ai-depth-main-integration-bb498256.zip),
SHA-256 `86f6a01600df10b7c37742b042016a366101fd40cee91a72dedce177a3162024`;
203,421 bytes, ten entries, every entry read back and hashed. Remote native
feature CI remains pending. Session/UI installation and the remaining issue
#74 acceptance are still open; this integration does not close the issue.

The first remote main-based check identified missing tool-inventory entries for
the two optional Python helpers. Both are now classified with explicit external
model/runtime requirements and invocation commands. The unchanged tool-policy
suite passes all 106 discovered tests locally (one existing Windows platform
skip); this metadata correction does not enable model downloads in CI or alter
the inference procedure. Remote checks on the corrected head remain pending.
