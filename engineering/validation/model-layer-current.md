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
