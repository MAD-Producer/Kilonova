# glTF linear blend skin checkpoint

Status: implemented on maintained main `7ba96e6e`; immutable-source
CPU/native/project/editor acceptance completed. Whole-workspace and
cross-platform gates remain separate. This document does not close #87.

## Import and sampling profile

Typed skins retain joint-node identity, optional skeleton and owned float MAT4
inverse binds with explicit identity defaults. Joint-associated matrices must
be finite and affine. Extra accessor elements also pass the reader's finite
profile. Sparse/range/stride/component/count rules use the bounded accessor
reader. Payloads, descriptors, aggregate joint references and influence-set
descriptors have explicit limits.

All consecutive paired JOINTS_n/WEIGHTS_n VEC4 sets are retained; influences
are not truncated to four. Joints use non-normalized unsigned byte/short.
Weights use float or normalized unsigned byte/short. Negative values, empty
sums, repeated active joints and out-of-range joints (even unused components)
are refused. Fully quantized sums use exact integer units; float sums follow
the public specification's recommended `2e-7 × active influence count` profile.
Only representational drift is normalized in f64 during deformation; arbitrary
invalid sums are not repaired.

Validation checks skeleton ancestry and joint common-root membership in each
referencing scene. Geometry maxima are computed once, avoiding repeated vertex
scans for shared mesh/skin bindings. `mesh_instances_with_skins_at` traverses
the hierarchy once and builds one metre-space palette per used skin; unused
skins allocate no palette and mesh instances do not clone palettes.

Morph precedes skin. Joint-world × inverse-bind matrices replace the mesh-node
transform for skinned primitives, including collapsed mesh-node matrices.
Unskinned primitives in mixed meshes keep their ordinary node transform.
Clip/time remain explicit; sampling never mutates source data or project keys.

## Native behavior and limits

Morph and skin share one immutable per-node/primitive/encode vertex buffer and
one admission charge. Source/frame and shared allocated-resource budgets both
apply; `deformation_bytes` reports the buffers. Pending frames retain independent
vertices, abandoned encodes retire leases, and displaced bounds drive global
blend sorting and actual hardware depth.

Normals use inverse-transpose blended linear matrices; tangents use forward
linear matrices. Directions normalize before interpolation. Tangent handedness
is preserved for supported positive determinants. Singular/reflected effective
skin transforms explicitly fail before target recording; ordinary reflected
world transforms retain existing support. Canceled directions and overflow
also refuse without bind-pose substitution. Deformation currently runs on the
CPU with per-encode uploads; no GPU-skin or fixed-frame-rate claim is made.

Skin errors retain node/mesh/primitive/phase and shared-pass
instance/request/source context through original typed renderer refusals. The
existing bounded frame failure observer and default Error logging route remain
in use; healthy frame logging is unchanged. Non-render scene validation retains
its existing text transport boundary.

## Executed acceptance

Final immutable-source gate `artifacts/pr-queue/issue87-skin-native-v1` passes
136 tests: 82 scene, 5 core model, 46 actual native/model, 2 original-error
boundary units and one actual desktop editor model-keyframe test. Zero failures
and zero ignores. Strict all-target scene Clippy, provenance, panel style
consumer checks and desktop FFmpeg/AI-depth check also pass. Every command
verifies unchanged source hashes. Tests cover four precisions and 1x/4x,
independent two-joint/morph pixels,
pending frames, nonuniform mapped normals/tangents, independent skins sharing
meshes, mixed primitives, real depth/global blends, one-byte resource refusal,
abandonment/recovery and unsupported-pose pixel preservation. Saved-project
tests reopen, remove the original asset, combine joint/layer keyframes and seek
backward without altering authored data.

Initial transparent-depth evidence is retained in
`issue87-skin-initial-native-v1`: its triangle had moved away from the sampled
centre. The corrected fixture centres the owning instance; expected colors,
depth equations, assertions and tolerances remain unchanged. The old malformed
`{}` skin fixture now asserts a typed missing-required-field error because skins
are implemented; valid and malformed skins have separate positive/negative
coverage. Existing assertions are not relaxed to obtain green.

Implementation and binary fixtures are independently authored from the public
[Khronos skin specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#skins).
No new dependency, external asset or copied implementation is added. No panels
or UI behavior is edited; the delivered movie is not rerendered. Android is
retired and excluded. Mixed mesh/plane/matte/effect/nested project composition,
calibrated AI depth, camera-selection UI and final #87 acceptance remain pending.

Evidence: [exact tested sources, raw outputs and failed fixture history](data/gltf-skin-native-v1.zip),
SHA-256 `d459ba56b6cbaec59460ebedb5093975d4cac69d9ecde5a27d39ab9b0b6c7cf7`; 1,300,281 bytes, 65 entries, all read back and verified.
Binaries/weights are excluded. This targeted gate does not attribute the
predecessor full regression or PR525 CI to these later morph/skin sources.
Stage totals overlap and must not be added.
