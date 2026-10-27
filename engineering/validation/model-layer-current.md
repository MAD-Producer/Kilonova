# Native editable model-layer milestone

Status: the authored desktop demo is accepted on `feat/87-model-layer`, based on
`3cef92882eae239433f2cf0a807d8d56ef1a0742` plus the source hashes in the demo
receipt. This is a development milestone, not completion of issue #87. Current
main integration and full regression are recorded separately below. Android is
out of scope.

## Delivered behavior

- Project model resources carry managed bundle paths, byte lengths and SHA-256
  identities. Model clips reference a scene, scale, pivot and sample count.
- The native renderer prepares owned scene resources before replacing a live
  catalog and rasterizes a model layer using the sampled camera and ordinary
  editable layer transforms. Healthy frame rendering does not reopen source
  files. Missing resources, invalid scene indices and identity mismatches fail
  explicitly.
- Model dependencies participate in history accounting, asset ID allocation,
  clipboard closure, package save/restore and movie source freezing. Portable
  packed output rejects a model that has not been explicitly baked.
- Native scene ownership can be stored with the session, while draw/encode
  reject use from a different thread before allocating native work. The
  allocation context retains its existing thread-bound contract.
- The demo uses independently authored OBJ/MTL geometry. It stores five position
  keys, four rotation keys and four scale keys, plus a camera layer. Its actual
  movie export is H.264/AAC, 960 x 540, 30 fps, 180 frames, six seconds.

## Evidence

The local evidence root is
`artifacts/pr-queue/issue87-keyframe-demo/`. `accepted.json` records modified
source hashes, output hashes, entry points, checks and limits. The editable
directory, `.motion` package and movie are independent output files. The movie
was produced by `motion_desktop_media::movie::FrozenMovie::export`, not by an
external rendering substitute. FFprobe confirms the dimensions, codec, frame
count and duration. All frames were decoded for the GIF preview.

`editor-native-presented/editor-model-{000,045,090,135,179}.png` are actual
Shell/GPU UI captures. Each follows a successful native surface Present. The
existing transform rows are expanded, so the saved keys and sampled values are
visible. These captures do not depend on OS desktop screen capture, which did
not provide usable window contents in this session. Earlier OS captures under
`editor/`, and captures made before presenting under `editor-native/`, are not
accepted model screenshots.

Targeted executions passed:

- Three core model-resource/history/storage/clipboard tests and three native
  composition/model tests in `issue87-model-layers-test-1.txt`
- Sixteen native scene GPU tests in `issue87-model-scene-gpu-test-1.txt`, including
  the original assertions and the added foreign-thread ownership regression
- The actual saved robot project in `issue87-model-editor-test-6.txt`
- The independent saved triangle fixture in
  `issue87-model-editor-default-test-1.txt`
- Desktop check and FFmpeg-enabled development build; actual movie export in
  `issue87-model-product-movie-3.txt`

The UI regression checks visible model pixels independently of blue selection
outlines, animation changes inside the composition viewport, unchanged authored
project state after seeking/capturing, and successful surface presentation.
Default-level file logs contain command and model-import Info start records.
The visible editor MCP session's stdout contains JSON-RPC messages only. Existing
missing Epic Vulkan overlay manifest errors are retained in file/stderr; they
did not prevent these model checks. No panel source files changed. The existing
DESIGN section 0.7 remediation items are retained outside this milestone.

## Remaining limits

- Meshes are rasterized in isolated model-layer passes. Different models and
  ordinary 3D planes do not yet share mesh depth or visibility resolution.
- This demonstrates whole-model transform keys, not skeletal, morph or imported
  model animation. Imported cameras and full scene lighting controls remain
  separate work.
- A user-facing model import command and asset-browser integration are pending.
- Remaining lifecycle/effect/nesting cases require broader review and
  regressions, including multiple model layers and shared depth.
- Full post-integration regression is pending; the targeted results do not
  substitute for it.

No issue closure or release readiness is claimed by this document.

## Main integration and model resource admission

The static-scene foundation and model layer have been replayed onto main
`eb2e8c8cc158beb37d539cc95d0a33b13f60332e`. The three foundation patches have
unchanged range-diffs. The model-layer conflict in packed output preserves both
independent refusals: native instance shader-data transport and unbaked native
models. The resulting model checkpoint is
`a1427f005292845aef0042fb4a3bbe192033ef06`; desktop check passes on that source.
The prior demo remains evidence for its original, explicitly hashed source.

Further source admission now bounds a catalog's aggregate retained scene
payload to 320 MiB as well as its encoded bytes. Each importer receives only
the remaining allowance before decoding; reused owned sources count too. This
is an accounted payload bound, not a process/decoder peak-RAM claim. A CPU
regression covers cumulative new imports, cumulative cache hits, exact-bound
acceptance and reuse after deleting the source file.

Material images, their mips and fallback/color-data variants join the shared
source texture pool with ordinary images, videos, effect resources and model
rasters. New scene image allocations enforce the remaining allowance before
native texture creation; lowering the pool also checks resident model pixels.
Depth/MSAA working targets remain in the shared native allocation budget.
Model mesh draws and uniform uploads join the frame statistics, and model
texture totals include material and depth/MSAA storage. Healthy frame accounting
uses retained capacities without allocating texture-observation metadata.

Local targeted runs `issue87-model-main-budget-test-4.txt` and
`issue87-model-main-scene-gpu-test-1.txt` pass 22 test executions with zero
failures and zero ignores. They include all sixteen native scene checks,
source-pool refusal for both new and resident models, recovery without changing
pixels, visible model keyframes, catalog failure preservation and CPU retained
payload admission. Full source-frozen regression remains pending.

## Full maintained workspace checkpoint

Executable-source head `b572d7e7b5320a8a74af8a5453f21674b7d0f60d` completes
the native workspace regression: 3,164 passed, zero failures, 14 existing manual
ignores across 311 test targets. The real editor keyframe capture, retained model
catalog admission, combined material texture budget and foreign-thread rejection
checks all execute and pass. Desktop check, provenance, panel style consumers,
FFmpeg binary build and actual JSON command catalogue also pass. The catalogue
contains `register_model_asset`; its stderr is empty. Android is excluded.

The first controller stops after successful tests because its ignore inventory
was copied from the earlier #78 checkpoint (12). The two additional entries are
manual expression-editor capture tests already present in merged baseline
`eb2e8c8cc158beb37d539cc95d0a33b13f60332e`. A separate continuation verifies
all fourteen ignored test files have identical Git blobs at that baseline and
the frozen model head, verifies the completed output hashes and source guard,
then performs the remaining binary/catalogue checks. No product/test source,
assertion, timeout or numerical bound changes to obtain this result. The original
controller failure and the baseline inventory audit are both retained.

Evidence: [frozen full regression, inventory audit and native editor captures](data/model-layer-b572d7e7.zip),
SHA-256 `7605dda879cb516dd644e71eab6488568ccb6c7ef19aa6b80ada0d2e8cfc7700`;
1,736,773 bytes, 78 entries, each read back and verified.

This checkpoint precedes rebasing the model commits onto the latest maintained
main. The combined regression after that rebase must include the optional
`motion-host/ai-depth` contracts as well as the native GPU and FFmpeg features.
Shared mesh/plane depth, imported skin/morph/animation/camera and the remaining
compositing acceptance stay open; the keyframe demo animates whole-model layer
properties rather than imported skeletal animation. This does not close #87.

## Maintained-main combined and final-source acceptance (2026-10-27)

This checkpoint supersedes the pending integration checks above. The frozen
combined source `653dc2056a2f12d0cdc6fbb6907d2a2a44b60376`, based on maintained
main `7bb54085242f2ca6d5eefe823a01b7ac7044239c`, passes 3,213 tests,
zero failures and 15 existing manual ignores across 317
targets, with native GPU, FFmpeg and optional AI depth features enabled.
Desktop check, provenance, panel-style guard, actual FFmpeg desktop binary build
and JSON command catalogue also pass. The catalogue includes
`register_model_asset` and produces no stderr. Android is excluded.

The original controller stops after successful tests because it carries the
previous checkpoint's 14-ignore inventory. Maintained main adds the manual native
character/paragraph capture test. An independent continuation verifies all
15 ignored test files have identical Git blobs at the frozen head and main base,
verifies source/log hashes and all named execution requirements, and performs
the remaining build/catalogue checks. The original controller, its stale
inventory failure and raw exit receipt are retained without changes. No test
assertion, timeout, numerical bound or ignore annotation is changed.

The final executable source is `06d1c71675de2e29a93d88bae520bdb341977169`.
It adds the later video colour fix to that frozen combined source. Its 31
affected contracts pass with zero failures/ignores; no-default host and native
FFmpeg/AI desktop checks and the exact-source native probe build also pass.
The frozen full workspace result is **not** represented as a full regression of
the later five changed Rust files. Remote merge-source CI remains separate.

Actual final-source Session/FFmpeg/CPU ONNX inference consumes exactly the sRGB
bytes accepted by the independent native colour-staging probe. The VFR request
uses requested 76,667 us and actual decoded 50,000 us. The pinned official
PyTorch reference has normalized maximum error 7.78702453293e-07 and mean error
1.20205951624e-07, within the unchanged bounds 2e-4 and 2e-5. The live missing-index
failure, explicit repaired recovery, stale-result invalidation and scratch
cleanup pass. With MOTION_LOG unset, Info start precedes failure, Error reaches
both file and stderr, and product stdout stays empty.

The separate final-source staging/rejection checks perform no inference: known
BT.709 tags are independently verified, native matrix maximum byte error is 3
(bound 4), sRGB reference error is 6 (bound 13), the signal-transfer LUT matches
exactly, and unknown metadata is rejected before backend entry. Those controlled
backend probes are distinguished from the actual ONNX execution above.

Evidence: [combined maintained-main regression and final-source native depth](data/model-depth-integration-06d1c716.zip),
SHA-256 `8359eddf745c936ce406ffbd48cbd5c498f4a267db0ebfc8ce407495ad8e3ddd`; 3,774,343 bytes,
192 entries, every entry read back and verified. The archive
contains sources, raw logs, receipts and authored fixtures; no model weights,
third-party source trees, native runtime binaries or executables are bundled.


Model layer transforms/keyframes, save/freeze/export and source budgets are
accepted for this stage. The already delivered model movie is not rerendered.
Shared mesh/plane depth, imported animation/skin/morph/camera and remaining
composition acceptance stay open under #87.

## Later #87 pipeline checkpoint

The subsequent [transform animation](gltf-transform-animation-current.md),
[shared native depth](gltf-shared-depth-current.md) and
[imported camera](gltf-cameras-current.md) checkpoints supersede the earlier
unsupported animation/camera boundary above. Their maintained-main full
regression and exact source-equivalence evidence are recorded in those ledgers.
Project mixed composition, skin/morph and calibrated AI depth remain open.
