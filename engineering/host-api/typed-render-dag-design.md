# Typed render DAG: implementation design

Status: work in progress for issue #78, not a delivery or acceptance claim.
Initial base: `40efcd35`. Issue #73 supplies the implementation-bound pointwise
proof and must be integrated before native grouping acceptance. No UI change,
Android device acceptance, or legacy migration is part of this delivery.

## Three representations

1. **Authoring data:** the saved Project, its ordered effect instances and
   `EffectImageInput` / track-matte references already describe image-level
   branches and merges. Embedded `.msgraph` documents describe shader-level
   arithmetic. Preserve these existing identities and persistence; do not add an
   unrelated second project graph just to demonstrate topology.
2. **Execution IR:** image values, typed ports and explicit operation boundaries,
   with stable origin IDs and code identities. Compile this from real sampled
   authoring data and resolved package implementations. Numeric values belong
   to frame inputs, not code keys. This representation owns optimization
   decisions, not GPU devices, queue submissions, or physical textures.
3. **Frame tasks/resources:** ordered preparation, materialization, encoding and
   composition tasks, sampled uniforms, negotiated image storage and admitted
   physical lifetimes. Lower the checked execution IR into the existing native
   plan and scratch allocator. Do not introduce a competing allocator.

The current renderer already deduplicates effected snapshots in dependency
order, preserves them outside atomic draw scratch, and restores those pixels
for composition. Keep that behavior; prove the new integration with actual
encoded producer work, not merely a topological helper's output.

## Semantic types

Use separate `ExecutionNodeId`, `ImageValueId` and origin identities (composition,
layer, effect, phase). An input references an image value and declares its port
role: primary image, original source, package resource, matte or backdrop.
Each output has exactly one producer and may have multiple consumers.

An image value carries the existing versioned `ImageContract`, pixel rectangle
and coordinate domain. Preserve all four precisions, color/alpha meanings and
numeric/depth meanings. Relative depth additionally needs an explicit near/far
direction; linear depth requires positive scene-distance semantics. Metadata is
not an implemented depth producer. Conversions are explicit operations; a
mismatched edge must not silently reinterpret a normal or depth value as color.
Physical storage is negotiated through the existing capabilities after checking
logical contracts; permitted channel padding does not change logical meaning.

An operation records time dependence, bounded halo, state/side effects, declared
cost and required capabilities. Resource lifetime and consumer count are derived
from edges and terminal consumers. Do not trust author-supplied pointwise flags:
only #73's actual implementation proof may authorize its initial fusion family.

## Optimization and identity

Numeric uniforms and sampled clocks update frame data without new portable or
native programs. Code, topology and static specialization change group identity;
unchanged groups retain their caches. Structure keys include code and semantic
contracts, exclude object identity and numeric values, and are bounded.

The current shader-node compiler embeds Float/Vec/Color constants in generated
code. Its PublicParameter path is already dynamic. #78 must address editable
numeric literals with an explicit bounded data layout and capability negotiation;
relabeling every numeric edit as structural does not meet the requirement.

Materialize shared values once. Multiple consumers are an explicit optimization
boundary until recomputation and error propagation are proved. Preserve unknown
effects, neighborhood sampling, coordinate/ROI/format changes, AI/3D, state and
side effects. No fusion may turn a neighborhood read into repeated evaluation
of its upstream graph. Keep original admission authoritative.

A local rounding tolerance is not a proof across an arbitrary consumer. In
particular, affected snapshots, mattes, transfer blends and adjustment backdrops
need explicit treatment. Preserve #73's all-precision alpha, UNORM16 transfer and
consumer boundaries until an end-to-end proof and native tests replace them.

## Scheduling, errors and runtime protocol

Validate bounded node/edge/port/cost counts before allocating traversal storage.
Reject duplicate producers, absent values, duplicate input roles, mismatched
contracts, unsupported capabilities, cycles and over-budget plans. Diagnostics
identify the origin, port, phase and measured limit; cycle context is bounded.
An iterative traversal avoids stack growth on long dependency chains.

CPU preparation may use a bounded worker pool for independent owned jobs. Return
bounded results to the owner; do not call a host logger while another thread owns
Session state. Native GPU work stays on its existing owner and uses batched
command encoding. Extra submits on one queue are not GPU parallelism.

Version the runtime plan and required capabilities. Exercise a real consumer's
acceptance and refusal, not only serialization round trips. No old-version
migration or Android device result is required. Release plans must not claim
unsupported history, async compute or depth production.

Keep ordinary cached frames quiet. Existing engine pre-handler starts/in-flight
state remain the worker hang signal. Compile failures preserve original causes,
use bounded caches and reach default file/Error-stderr output after business
locks are released; stdout stays reserved for the protocol.

## Required native acceptance

- Persist/reopen the authoring relationships used by the actual IR lowerer
- Compare grouped/ungrouped real pixels for all four precisions, including alpha,
  color boundaries, retained quantization and downstream consumers
- Count actual shared producer encodes in a diamond; add a cycle, format mismatch
  and budget refusal with exact diagnostic context and successful recovery
- Prove numeric edits add no compilation, and a structural edit recompiles only
  affected groups using actual portable/native counters
- Measure code size, compile costs, passes, admitted peak resources and frame
  intervals; distinguish logical traffic from unavailable physical bus counters
- Test runtime capability acceptance/refusal and actual default failure outputs
- Run appropriate full regression and cross-platform CI before closing #78

No unconditional single-pass, unlimited graph size, fixed frame rate or relative
product-performance claim follows from this design.
