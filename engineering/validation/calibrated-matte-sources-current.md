# Independent calibrated matte-source geometry (#87)

Maintained desktop increment after calibrated receiver mattes. This is part of
the open 3D delivery; it does not close #87 or implement #82's render/cache path.

## Behavior and ownership

A calibrated image/video used only by calibrated receivers can now supply an
independent projected matte image. Its real calibrated source mesh, camera,
source-local mask, opacity and already projected child matte are rendered into
a private working-color target with native depth and 4x MSAA. Source visibility
and layer transfer do not replace the independent source image. A visible
eligible source also remains in the main physical depth run.

Dependencies are visited iteratively and deduplicated by sampled source ID,
with storage bounded by the scene's layer count. Calibrated chains read their
children's completed private targets. Ordinary leaves keep existing retained
materialization. Hidden consumed calibrated dependencies are removed from the
post-depth flat scene, so an invisible input does not become a stale combined
raster or split a visible physical run.

One owner-thread native pass caches depth/MSAA attachments and bounded pipeline
variants; clear, source-catalog replacement, target eviction and resource-ledger
collection include that pass. Each encode owns its projected color and source
coverage. Native allocations join shared admission; projected colors and mask
coverage also join source-texture budget checks. Errors preserve the original
typed cause and propagate through the existing worker default log boundary.

Independent source passes are submitted before the caller's physical geometry
commands. This adds submissions and is not a submission-fusion or sustained-P95
claim. Pending commands must never share mutable projected colors or masks.

## Independent GPU controls

- Four working precisions and all four matte modes compare interior pixels with
  analytic rays through the edited camera, the piecewise calibrated red surface,
  ordinary blue plane and a separate uniformly near calibrated source
- The source is off the reference camera's optical axis. Reference unprojection
  shifts its local X centre by -2, in addition to scaling XY by 0.8 and moving
  local Z to 20; its saved reference geometry remains fixed after camera edits
- Two hidden calibrated sources form a masked source chain. Opposite source
  masks and inner matte modes must exchange independent red/blue test rays;
  reverse submission preserves both complete independently captured references
- Failed native admission preserves the prior completed front and original
  resource-rejection type; recovery, abandoned outer commands, retirement and
  asset eviction/repreparation preserve the expected images and admitted bytes
- A visible calibrated green matte behind the opaque blue plane must remain
  physically behind it while still supplying its independent alpha image

The source-bound related gate passed 149 distinct tests, zero failed and zero
ignored: nine native render integration targets (121 tests), compositor (2),
mask GPU (14) and host scheduler (12). Desktop FFmpeg/AI-depth compilation,
style-consumer and provenance checks pass. Every source/configuration hash
remained unchanged after every step. This is related local acceptance, not
final umbrella full-workspace/current-main/cross-platform acceptance.

## Failure evidence and intentional test change

The old implementation ran the new GPU source test and refused object=91,
source=8, phase=shared_geometry. This is a real missing-path result, retained in
`artifacts/pr-queue/calibrated-matte-source-red-v3.log`. Earlier v1 was a local
compiler-environment setup failure; v2 was a new fixture missing an unreferenced
invisible layer from its reference-only sample. Neither proves a product fault.

The initial green v1 run caught an error in the new independent oracle: it
assumed uniform near-depth shrinkage occurred about the layer centre. The
reference camera's optical axis is offset from that centre. The analytic local
X correction was fixed without changing the expected linear-light color or
precision tolerances; green v2 passed all precision/mode/camera combinations.
The subsequent new chain fixture's closure needed an explicit usize index;
that compile error is retained separately from GPU failure evidence.

The previous blanket unsupported calibrated-source guard now exercises an
effected calibrated source, with the same original object/source/error-cause and
completed-front assertions. Plain calibrated sources are supported by the new
independent geometry path and its actual pixel controls. General calibrated
image effects still require a separate source/geometry contract. The original
ordinary-receiver rejection test remains unchanged.

## Remaining limits

Calibrated sources feeding ordinary receivers or effect inputs, general
calibrated image effects, processed visible ordinary spatial sources, nested and
exposure depth, intersecting translucent ordering, DOF, long-duration real-codec
playback/P95, final normative comparisons and current-main full-workspace plus
maintained-desktop cross-platform acceptance remain pending. Synthetic RGBA and
small native scenes do not establish codec throughput or final issue closure.
Android is retired and excluded. No panel/UI, dependency or external asset
changes are included.

Receipt: `artifacts/pr-queue/calibrated-matte-source-regression-v1/accepted.json`.
Archive: `data/calibrated-matte-sources-native-v1.zip`; CRC and every entry verified against originals.
SHA-256: `ae0d685eedeeb4dd4efa94c5a24ada3ec0649c8bf058e5ebac4ba062ba39477a`.

## Owned video source color revisions

The calibrated source now also has a native owned-video control. The registered
codec path does not exist; decoder inventory must remain empty when a sampled
source owns exact display pixels. Two explicit incoming RGBA revisions at the
same source PTS produce independent analytic green/red-luminance mixtures at a
near red ray and unchanged blue at a far red ray. Repreparing the second source
and submitting pending commands in reverse preserves both complete references.

After both valid revisions, a sampled source with geometry but no owned display
pixels must refuse with object=91/source=8 even while the previous video pixels
remain resident. The completed front must remain unchanged. Full eviction and
repreparation must recover both valid references. These synthetic incoming
samples prove native color ownership and refusal, not actual codec decoding,
indexed playback throughput, long-duration behavior or final #87 acceptance.

The focused source-bound gate passes 60 tests plus desktop compile/style and
provenance checks. All production/configuration hashes match the broader
149-test gate; only the test file changed. Fifty-nine tests overlap, giving
150 distinct related tests across these gates, not 209. Equivalence receipt:
`artifacts/pr-queue/calibrated-matte-source-video-v1/equivalence.json`.
Archive: `data/calibrated-matte-source-video-native-v1.zip`; CRC and every entry verified against originals.
SHA-256: `a8e62392c4e035f6587183977496e63688dfc41d98a02233c24c3baba24d00dc`.
