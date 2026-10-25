# Typed render DAG: current delivery ledger

Issue #78 is **in progress**, followed by #74 and #87. This is a partial
implementation record, not an issue-closure claim. The initial foundation was
based on `40efcd35`, then integrated with #73 at `ebb983c0`; the foundation
commit is `6fdc5f1d`; the native snapshot milestone is `7fe37354`. PR #491
passed all required checks and merged as `7bd4d860`, with its actual merge
subject/body verified in Chinese and issue #73 closed. No other agent's PR diff
was reviewed and no subagent was used.

## Requirements and actual coverage

| Item | Current evidence | Remaining requirement |
| --- | --- | --- |
| 78-R1 authoring / execution / frame ownership | Existing Project relationships serialize/reopen through the actual model; new tests lower a saved authoring diamond into snapshot IR | Complete effect-level lowering before physical placement and disk/package acceptance; a JSON round trip is not full persistence acceptance |
| 78-R2 typed values and semantics | Versioned image values, origin IDs, distinct input roles, canonical color/alpha/precision, coordinates, rectangles, depth-direction rules, time, halo and state/capabilities | Actual non-color/depth producers belong to later work; metadata is not a producer |
| 78-R3 checked topology and lifetime | Iterative bounded topology/cycle checks, unique producers, exact edge types, explicit terminal consumers, lifetimes and cost limits | Use the complete execution IR for all operation scheduling and grouping; current native use is atomic effected snapshots |
| 78-R4 actual shared upstream work | Four native precisions times two stack orders: actual encoded/copy trace has exactly one entry for each of three snapshots, including their shared upstream; exact pixels across stack orders | Extend to complete effect-level branch/merge scheduling and retained boundary cases |
| 78-R5 numeric data / code identity | Actual worker-owned portable compiler cache has two entries / 8 MiB code bounds; private literal/default/folded data updates reuse actual compiler output; native preview creation calls remain zero while uniform/overflow pixels change, with actual worker/engine/capture verification | Per-instance bound-layer updates without runtime-package churn, native effect pipeline counts and affected-group structural acceptance remain unfinished |
| 78-R6 proven grouping | #73 implementation-bound opt-in proof is present; its seven native tests and procedural alpha guard pass through the new scheduler | Connect grouping decisions to the complete IR and preserve all proven refusal boundaries |
| 78-R7 CPU / GPU scheduling | Native snapshots keep the same GPU owner, command encoder and atomic scratch allocator; no new per-node device or submit | Bounded parallel CPU preparation and measurements remain unfinished |
| 78-R8 runtime version / capabilities | Logical graph version/capabilities validate; native snapshot consumer refuses missing ImageDagV1 and insufficient budget; actual package loader refuses invalid numeric-data versions/capabilities/capacities/payloads | Versioned packaged execution plan and real portable execution-IR consumer acceptance/refusal remain unfinished |
| 78-R9 failures / observation | Original typed graph errors survive renderer context; bounded cycle layer IDs; actual malformed cached WGSL and invalid private data reach default Info start / Error file+stderr without stdout, retaining old preview pixels and recovering; candidate budget refusal is tested on GPU | Exercise complete execution-IR cycle/type/budget failures through the same production output channels |
| 78-R10 performance / admission | Existing physical scratch/source admission stays authoritative; logical graph caps are additional and bounded | Measure end-to-end shader/compile/pass/frame/peak costs, including per-frame code identity hashing; no performance improvement is claimed |
| 78-R11 regression / CI | Current targeted library/native/affected-path results below pass with source snapshots | Full integrated final-source regression, cross-platform CI and final acceptance remain pending |

Android device acceptance and legacy migration are outside the user-confirmed
scope. Runtime version and capability refusal remain required. No UI, panel,
style, new dependency version or third-party asset change is included in this milestone.

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

## Editable numeric data milestone

The [numeric data contract](../host-api/editable-graph-data.md) documents the
actual live worker routing, separate baked compilation API, uniform tail and
bounded f32 byte-container overflow. Independent preview and native effects
consume those values; private data adds no public controls. The package loader
checks capability/version consistency, capacity, dimensions and numeric values.
The compile report counts encoded resource bytes, not GPU resident memory.

Receipts, selected source snapshots and actual 24-row GPU results are retained
in the [numeric evidence archive](data/typed-render-dag-numeric-7fe37354.zip).

| Receipt under `artifacts/pr-queue/` | Actual result |
| --- | --- |
| `issue78-numeric-transport-v1` | Failed test compilation from a moved Option in the new oracle fixture; no test pass claim |
| `issue78-numeric-transport-v2` | Four new tests passed; 24 exact native/baked pixel rows across four precisions, both data carriers and three colors |
| `issue78-numeric-transport-v3` | Four tests and the same 24 GPU rows passed after cause-preserving image errors and corrupt dimension/NaN rejection coverage |
| `issue78-numeric-integration-v1` | Desktop check passed; complete graph tests 77 passed and effects library 26 passed, zero failed/ignored in those scopes; desktop unit scope 207 passed/one failed plus one integration pass |
| `issue78-numeric-integration-v2` | Corrected desktop check passed; desktop node scope 210 passed, zero failed/ignored, including the real background-worker data/undo test and independent preview pixel/allocation tests |

The first desktop failure was an existing onboarding assertion that a changed
Color changes baked shader source. Routing ordinary `compile()` to editable
compilation had violated that API contract. The assertion is unchanged. The
correction preserves baked compilation and gives the actual live worker
explicit runtime entry points. The new worker test checks both contracts and
undo recovery. No value was injected into shader text to satisfy the assertion.

Graph/effects results retain their pre-routing-correction source scope; their
production graph/effects source did not change in that correction. The second
desktop run covers the corrected worker/preview routing. None of these runs is
a final integrated workspace or cross-platform CI result. At this transport
milestone, numeric edits still compiled shaders and replaced runtime packages
and pipelines. The following milestone adds owned compiler and independent
preview reuse; bound runtime packages and native instances still remain before
claiming #78's complete no-recompile criterion.

## Owned compiler and independent preview reuse milestone

The actual node compiler thread now owns a two-entry, 8 MiB UTF-8 code cache.
It compares complete generated source and retains private compiler results;
returned mutable shaders cannot authorize or poison later hits. Every request
still validates its graph, manifest, private values and current budgets. Actual
portable compiler calls and retained code bytes are exposed in the report.

The actual `NodeInstall` path creates a transactional admitted preview candidate
and shares successful native pipelines only under the same private allocation
owner and actual WGSL/static format. Source/data/output/uniform/presentation
resources remain separate. A bounded current-plus-one-alternate presentation
cache handles native swapchain / PNG format round trips. Three-format eviction
and cross-owner refusal have actual GPU creation-call checks. This is pipeline
reuse, not driver timing, resident-memory or universal performance evidence.

Receipts, final-source hashes, selected source, actual 24-row native data results,
four zero-build preview pixel rows and default production failure outputs are
retained in the [verified reuse archive](data/typed-render-dag-reuse-72a521ea.zip).
The archive has 107 verified files plus its manifest, 560,662 bytes, SHA-256
`582e2efe245728857eb87ef8d3f1be4cbae4ff11e1330b01ee700ec3bb4c162e`.

| Receipt under `artifacts/pr-queue/` | Actual result |
| --- | --- |
| `issue78-numeric-reuse-v1` | Desktop check passed; graph scope 79 passed / one failed / zero ignored; the new input-default fixture omitted its required graph format version |
| `issue78-numeric-reuse-v2` | Desktop check, complete graph 80 and effects library 26 passed; desktop top-level scope 211 passed / one failed / zero ignored; a child zero-build assertion exposed actual swapchain/PNG presentation-format churn, not a second independent failed test |
| `issue78-numeric-reuse-v3` | Corrected desktop check and node scope 212 passed / zero failed / zero ignored across six targets, including actual compiler -> engine -> capture and default fault/recovery output acceptance |

No previous assertion was weakened. The fixture now declares the format it
uses. The actual engine zero-build assertion remains zero; the implementation
retains the two owner-local presentation formats instead. The corrected desktop
gate covers both the default file/Error-stderr/stdout test and real allocation
refusal, malformed shader/data, old-candidate pixel preservation, paused output
invalidation, new-code rebuild, format eviction and cross-owner checks.

The v2 graph/effects source hashes match the corrected final source in those
crates. The v3 desktop gate covers the changed display implementation and its
round-trip tests. The archive verifies both scopes explicitly; neither is a final
integrated workspace or cross-platform CI result. A source-bound archive helper
initially used the Windows default GBK decoder; its UTF-8 correction changed no
tested source or assertion, and the completed archive was read back and hashed.

Bound-layer application still installs value-dependent packages and can clear
native effect pipelines. Diagnostic bound canonical validation also still builds
a separate preview. Instance data, complete pre-placement effect IR, packaged
execution-IR consumer negotiation, complete costs/peaks and final integration/CI
remain pending. Issue #78 stays open, followed by #74 and #87. No panel file was
changed, no other agent's PR was reviewed and no subagent was used.
