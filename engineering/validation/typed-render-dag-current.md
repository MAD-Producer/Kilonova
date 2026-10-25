# Typed render DAG: current delivery ledger

Issue #78 is **in progress**, followed by #74 and #87. This is a partial
implementation record, not an issue-closure claim. The initial foundation was
based on `40efcd35`, then integrated with #73 at `ebb983c0`; the foundation
commit is `6fdc5f1d`. PR #491 still needs its final Windows check at the time
of this record. No other agent's PR diff was reviewed and no subagent was used.

## Requirements and actual coverage

| Item | Current evidence | Remaining requirement |
| --- | --- | --- |
| 78-R1 authoring / execution / frame ownership | Existing Project relationships serialize/reopen through the actual model; new tests lower a saved authoring diamond into snapshot IR | Complete effect-level lowering before physical placement and disk/package acceptance; a JSON round trip is not full persistence acceptance |
| 78-R2 typed values and semantics | Versioned image values, origin IDs, distinct input roles, canonical color/alpha/precision, coordinates, rectangles, depth-direction rules, time, halo and state/capabilities | Actual non-color/depth producers belong to later work; metadata is not a producer |
| 78-R3 checked topology and lifetime | Iterative bounded topology/cycle checks, unique producers, exact edge types, explicit terminal consumers, lifetimes and cost limits | Use the complete execution IR for all operation scheduling and grouping; current native use is atomic effected snapshots |
| 78-R4 actual shared upstream work | Four native precisions times two stack orders: actual encoded/copy trace has exactly one entry for each of three snapshots, including their shared upstream; exact pixels across stack orders | Extend to complete effect-level branch/merge scheduling and retained boundary cases |
| 78-R5 numeric data / code identity | Numeric effect changes keep actual shader code identities; adding a pass changes only the affected atomic draw's identity | Editable `.msgraph` literals are still embedded in WGSL; no end-to-end no-recompile claim for graph edits yet |
| 78-R6 proven grouping | #73 implementation-bound opt-in proof is present; its seven native tests and procedural alpha guard pass through the new scheduler | Connect grouping decisions to the complete IR and preserve all proven refusal boundaries |
| 78-R7 CPU / GPU scheduling | Native snapshots keep the same GPU owner, command encoder and atomic scratch allocator; no new per-node device or submit | Bounded parallel CPU preparation and measurements remain unfinished |
| 78-R8 runtime version / capabilities | Logical graph version/capabilities validate; actual native snapshot consumer refuses missing ImageDagV1 and insufficient node budget with typed causes | Versioned packaged execution plan and real portable consumer acceptance/refusal remain unfinished |
| 78-R9 failures / observation | Original typed graph errors survive renderer context; cycle diagnostics identify bounded actual layer IDs; success after each refusal is checked | Exercise new graph failures through default production file/Error-stderr logging and stdout isolation |
| 78-R10 performance / admission | Existing physical scratch/source admission stays authoritative; logical graph caps are additional and bounded | Measure end-to-end shader/compile/pass/frame/peak costs, including per-frame code identity hashing; no performance improvement is claimed |
| 78-R11 regression / CI | Current targeted library/native/affected-path results below pass with source snapshots | Full integrated final-source regression, cross-platform CI and final acceptance remain pending |

Android device acceptance and legacy migration are outside the user-confirmed
scope. Runtime version and capability refusal remain required. No UI, panel,
style, dependency or third-party asset change is included in this milestone.

## First native execution consumer

`PlanBuilder::snapshot_execution` lowers actual sampled/frame relationships into
a typed image DAG and obtains its checked order. `Renderer::encode_effects`
uses that order for real snapshot preparation and encoding. This replaces the
native caller's recursive dependency traversal; the existing standalone public
ordering helper remains for its current callers/tests. Ordinary frames without
snapshot readers take the empty path before constructing graph/capability data.

Each current IR node is an **atomic effected draw**, with a canonical linear,
premultiplied image output, actual layer identity and output raster extent. Its internal
passes, bindings and scratch reuse retain the existing native owner. Its real
WGSL/GLSL code produces its code digest; numeric uniforms and origin IDs do not
enter that digest. This coarse native consumer is not the final effect-level
execution lowerer. It currently derives from the existing frame prototype after
planning, so it does not establish complete pre-placement IR ownership.

Raw-source readers bypass effected snapshots. Mattes request the source's raw
effected image; composition applies the matte afterward. Explicit terminal
consumers keep snapshots separate from reused draw scratch. The existing
allocator remains the physical resource authority. Logical lifetime ordinals
are not physical allocations or peak memory measurements.

The SDK's `loop_work` metric does not prove a texture-sampling bound. Unknown
sampling costs are represented as `None`, propagate to the aggregate, and
cannot pass a policy requiring proved bounds. A declared zero stays distinct.
Default snapshot checking preserves existing shader admission and reports an
unknown bound; it does not claim zero sampling work or unlimited admission.

`Renderer::snapshot_encodes` records successful actual pass encoding and
snapshot copy calls for the latest **local composition attempt**, bounded by
MAX_LAYERS. It is cleared for new attempts, including direct/no-effect paths.
It is not an aggregate across nested compositions, a submission-completion
signal, or a prediction made from graph node counts. The native test completes
readback and compares pixels after checking that trace.

## Source-bound verification

Receipts and selected source snapshots are retained in the
[verified evidence archive](data/typed-render-dag-snapshot-6fdc5f1d.zip), with a
SHA-256 manifest checked after archive creation. Earlier counts keep their
historical source scope; subsequent implementation is not attributed to them.

| Receipt under `artifacts/pr-queue/` | Actual result |
| --- | --- |
| `issue78-foundation-logical-v1` | Failed compile: Rust needed explicit usize types for iterative cycle tracing; no test pass claim |
| `issue78-foundation-logical-v2` | Seven logical IR tests pass before integration with #73 |
| `issue78-foundation-integrated-v1` | Combined effects library at `6fdc5f1d`: 25 passed, zero failed/ignored; no native integration claim |
| `issue78-foundation-snapshot-v1` | Four new native-consumer tests pass; eight actual GPU precision/order rows with exact stack-order pixels; cycles/budgets/capability refusal and recovery; real code identity/data checks |
| `issue78-foundation-integrated-v2` | Updated effects library: 26 passed, zero failed/ignored, including unknown/zero/proved sampling-cost separation |
| `issue78-foundation-snapshot-regression-v1` | Seven affected targets: 36 passed, zero failed/ignored; image-order allocation contracts, layer compositing, masks, image effects, phase adjustments, pointwise fusion and procedural alpha |

All acceptance runs kept their source unchanged. No existing assertion or pixel
bound was weakened. This is targeted affected-path evidence, not a full desktop,
workspace, portable-runtime, cross-adapter or issue-completion result.
