# Imported transform-animation checkpoint (#87)

## Scope

This stage adds independently authored glTF 2.0 translation, rotation and scale
clips. STEP, LINEAR and CUBICSPLINE follow the
[Khronos specification, including Appendix C](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html).
Linear rotations use shortest-arc quaternion interpolation. Cubic tangents use
the interval in seconds and quaternion results are normalized without changing
the authored tangent signs. Sampling clamps each channel at its endpoints.

`Scene::animations`, `sample_animation` and `instances_at` expose explicit clip
selection and sampling. Static `instances` retains its previous rest-pose
behavior. Authored matrices on unanimated nodes survive without decomposition.
Transforms use the original right-handed, Y-up metre hierarchy and TRS order.

The optional project `ModelClip.animation` stores a clip index, offset in
seconds and signed playback speed. Layer-local frames and project FPS determine
the imported time. Zero speed holds a pose; there is no implicit looping,
autoplay or clip blending. Native `SceneFrame` receives the sampled time.
Ordinary layer Euler tracks, including multiple revolutions, keep their existing
representation and interpolation.

## Validation and limits

Timestamp accessors are floating-point SCALAR with matching min/max, finite,
nonnegative and strictly increasing. Output shape/count must match the channel
and interpolation. Rotation accessors support float and normalized signed or
unsigned 8/16-bit storage. Unit-length validation allows the corresponding
quantization error; values are normalized and cubic tangents are preserved.
Sparse accessors use the existing checked sparse reader. Animation views cannot
declare GPU vertex/index targets or a byte stride.

The total decoded channel key count has an explicit import limit. Owned channel
copies, including shared samplers and separate clips, charge the retained-byte
budget. Clip/channel/sampler descriptors and names are bounded. Duplicate
node/property targets, animated matrix nodes, invalid references, malformed
data and non-finite or zero-quaternion sample results return typed errors.
Import failures use the existing Info-before-resolver and Error-with-source
logging contract; successful animation frames add no log IO.

The model catalog checks selected clip indices before replacing owned sources.
A fully collapsed animated hierarchy is an empty visual. Other singular
transforms still receive the explicit native shading refusal. No retry, layer
Euler conversion or implicit fallback is introduced.

## Acceptance state

The CPU scene checkpoint passes 60 tests: 3 frozen-bundle, 22 glTF import,
20 OBJ import and 15 new transform-animation tests, with zero failures or
ignores. The new cases cover explicit selection, endpoint and exact-key
behavior, antipodal rotations, cubic time scaling, overflow/recovery,
normalized integer rotations, hierarchy, sparse data, owned frozen roundtrip,
invalid payloads and budgets. The first compile exposed a missing Deserialize
derive on the private rest-transform type; it was corrected before this run.

The old unsupported-feature loop now covers skins/cameras; transform clips
are an implemented capability. Its Unsupported assertion is unchanged. A new
test separately rejects malformed animation JSON and unsupported animation
targets. Existing numerical bounds, deadlines and runtime test assertions are
unchanged.

The final source-frozen targeted gate passes **88 tests, zero failures and zero
ignores**: the 60 scene tests, 5 core model tests, 22 native model/scene tests and
the desktop editor model test. Provenance and the FFmpeg/AI desktop check also
pass. Every executable-source hash remains unchanged through all steps.

The actual native composition renderer loads the saved owned glTF bundle, then
renders after deleting its source file. Imported translation contributes ten
pixels while the independent layer position track contributes thirty; alpha
centroids verify the combined forty-pixel displacement. Random seeks reproduce
the earlier pixels exactly. Missing clip selection reports layer/source/clip
IDs before committing the catalog and preserves the prior visible source. A
held zero-scale clip is transparent and selecting the original clip recovers
the original pixels. Existing PBR, texture precision, material, foreign-thread,
budget and static-model tests also execute. These pixel tests use native GPU
rendering, not mocked successful returns.

Storage and clipboard retain clip selection without converting 720-degree layer
Euler keys. A separate sampled-scene test verifies layer timeline offsets,
signed speed and project FPS determine imported time. The real editor child
test verifies existing whole-model keyframes and UI captures after this change;
it does not rerender the previously delivered model demonstration movie.

Evidence: [source hashes, patch, raw outputs and native editor fixture](data/gltf-transform-animation-native-v1.zip),
SHA-256 `3333e7fae52b007b690c74b587590885c6763b0fad33f07a23c43e5021e4ab79`;
1,224,397 bytes, 55 entries, each read back and verified. The gate tests an
uncommitted source snapshot based on `06d1c716`; the archive includes exact
tested-source hashes/files and the executable-code diff for reconstructing it.
This is targeted local acceptance, not a new full workspace or cross-platform
CI claim. PR #522 contains the preceding model/depth milestone, not this later
animation change. Android remains excluded.

## Remaining work

This does not complete #87. Skinning, morphs, imported cameras, unified mesh/plane
depth, calibration and the remaining composition acceptance are separate work.
There is no new clip-selector UI in this stage. Android is retired and excluded
by user instruction. Fixtures contain only authored geometry and binary data;
no third-party assets, dependencies or product artwork are added.
