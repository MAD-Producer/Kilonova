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
| 78-R1 authoring / execution / frame ownership | Actual model relationships lower to snapshot IR; exported graph packages now carry a reusable image execution template, checked by the real loader and instantiated by the native consumer | Complete initial semantic ownership before CPU preparation and transactional layout proposals; package templates do not reconstruct authoring graphs |
| 78-R2 typed values and semantics | Versioned image values, origin IDs, distinct input roles, canonical color/alpha/precision, coordinates, rectangles, depth-direction rules, time, halo and state/capabilities | Actual non-color/depth producers belong to later work; metadata is not a producer |
| 78-R3 checked topology and lifetime | Checked effect-level operations control actual snapshot/native task coverage; checked package-local DAG order and lifetimes control native pass order, both sampled image ports and working-slot placement | Complete initial semantic lowering and typed external-import coverage; outer effect lowering still follows CPU preparation and native drafts |
| 78-R4 actual shared upstream work | Four-precision native snapshot traces retain one encode/copy per shared producer; four-precision package-local diamonds execute A/B/C/D despite a B/D/A/C shader list, with actual draw counts, an independent numeric oracle and a differing linear-list control | Complete initial effect-graph ownership and final integrated boundary acceptance |
| 78-R5 numeric data / code identity | Numeric updates preserve validated packages and native pipelines; four-precision uniform/overflow instance tests verify 16 raw-pixel updates and eight disk reopen/isolation/undo cases; structural edits rebuild only the affected program; grouping preserves actual generated code and runtime manifests | Complete IR-level affected-group scheduling remains unfinished; cache eviction can require recompilation |
| 78-R6 proven grouping | #73 implementation-bound proof, local SSA def/use closure and effect-level IR primary-chain/unique-consumer checks jointly gate actual groups before final physical placement | Complete semantic import/boundary coverage and final acceptance; closed graph regions alone never establish shader equivalence |
| 78-R7 CPU / GPU scheduling | Pure vector tessellation now runs in bounded batches of at most four jobs with ordered global-budget/cache admission; native GPU ownership, encoder and atomic scratch allocator stay unchanged | Mask/generator state remains serial; end-to-end costs and final integrated scheduling acceptance remain unfinished |
| 78-R8 runtime version / capabilities | Version-1 package image templates require image_execution_v1, bind actual compiled code and current precision/extents, and execute natively; actual loader tests reject versions/capabilities/code/cycles and recover; the targeted source-bound runtime gate passed | Complete final desktop integration/CI; extending the retired Android/GLES packed ABI is excluded by user instruction |
| 78-R9 failures / observation | Typed cycle, precision mismatch and pass-budget failures now traverse the actual worker and default file/Error stderr channels with request/origin/timing context, stdout isolation, original GPU pixels and same-renderer recovery; earlier compiler/private-data faults remain covered | Confirm the complete final source in G integration and cross-platform desktop CI |
| 78-R10 performance / admission | Physical scratch/source admission remains authoritative; a bounded weak code-identity cache avoids rehashing unchanged shader text on numeric rebuilds | Measure end-to-end shader/compile/pass/frame/peak costs; hash counters are not frame timing or unique driver allocation measurements |
| 78-R11 regression / CI | Current targeted library/native/affected-path results below pass with source snapshots | Full integrated final-source regression, cross-platform CI and final acceptance remain pending |

All Android implementation and acceptance, plus legacy migration, are outside
the user-confirmed scope. The user explicitly reconfirmed on 2026-10-25 that
Android is no longer maintained. Runtime version and capability refusal for
maintained desktop consumers remain required. Frozen packed-v12 limitations are
recorded boundaries, not desktop completion blockers. No UI, panel,
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

At that milestone, bound-layer application still installed value-dependent
packages and diagnostic canonical validation built a throwaway preview. The
following instance-data milestone addresses those two paths. Complete
pre-placement effect IR, structural affected-group preservation, packaged
execution-IR consumer negotiation, bounded parallel preparation, complete
costs/peaks and final integration/CI remain pending. Issue #78 stays open,
followed by #74 and #87. No panel file was changed, no other agent's PR was
reviewed and no subagent was used.

## Bound instance-data and canonical proof milestone

Numeric bound updates now retain the same validated package Arc, archive and
native effect pipeline. The model and sampled scene share bounded, immutable
typed payloads; native overflow has a current-plan-only admitted binding cache.
Project format 27 persists the override explicitly. Old-project migration is
outside the agreed scope. The existing serde `rc` feature was enabled without
changing a dependency version; provenance files were checked and no asset added.

Canonical diagnostic validation retains only successful owner-local pipelines
and bounded code/display variants. Candidate textures/groups/buffers retire
after validation. A real GPU test keeps that proof alive, waits for submission
completion, checks admitted allocations return to baseline and then checks zero
pipeline creation calls when building from the proof. This excludes driver
pipeline storage from its allocation claim.

The [verified instance archive](data/typed-render-dag-instances-16642c71.zip)
contains 250 files plus its manifest, 1,214,955 bytes, SHA-256
`2f7d98070af4e5463d6926f077c564e3204a95fe95e63b87dc255c4f927fc807`.
Every archived entry and final Rust/Cargo source hash was read back and checked.

| Receipt under `artifacts/pr-queue/` | Actual result |
| --- | --- |
| `issue78-numeric-instance-v1` | Failed compilation: new preparation helper omitted the effects Error conversion |
| `issue78-numeric-instance-v2` | Failed compilation: new scene comparison omitted Arc qualification |
| `issue78-numeric-instance-v3` | Desktop check and three payload-related tests passed; bound GPU test passed with four numeric rows and two same-package-instance isolation cases |
| `issue78-numeric-instance-v4` | Desktop check, payload scope three and complete model scope 74 passed; desktop unit scope 216 passed / one failed plus one integration pass; new packed-plan fixture omitted reserved white resource zero |
| `issue78-numeric-instance-v5` | Final minimized source: desktop check, payload scope three, model scope 74 and desktop node scope 218 passed, zero failed/ignored; six desktop test targets include filtered empty targets, which are not additional passes |

The packed fixture now supplies reserved resource zero; its explicit refusal
and success assertions are unchanged. Unrelated model formatting was removed
under exact canonical-rustfmt equality, recorded in the archive. No previous
assertion was weakened, no test ignored and source remained unchanged during
each gate. New payload tests cover bounded deserialization, combined capacity,
invalid version/NaN/layout rejection, uniform tail isolation and signed-zero
bit preservation.

The bound matrix compares actual native output against separately validated
baked packages for private uniform and overflow data. Four numeric changes keep
package pointer/hash, portable compile count and native pipeline count stable;
an unchanged next frame uploads zero private texture bytes. Two same-package
layers retain independent data and bindings, and undo restores original pixels.
This matrix is Unorm8. Earlier four-precision transport rows remain separate
evidence, not all-precision per-instance acceptance.

A separate actual background compiler -> Engine install -> canonical/preview
validation -> bound instance -> native renderer test verifies stable package
identity and four actual native pipelines across four changed pixel results.
Warm canonical/preview candidate build deltas are zero. Corrupt canonical WGSL
and layout reach default Info starts and two Error records with operation,
request, elapsed time and causes in both file and stderr; stdout remains clear.
The live instance, native output and build count survive each failure and a
later valid edit recovers. Actual storage save/load restores the embedded
package plus updated instance payload and reproduces the saved native pixels.

Native GPU accounting includes private textures and any retained package
defaults; source admission includes old plus candidate capacity. Every encoded
use retains admission through submission completion, including replaced caches.
These byte/upload counters are not total RAM, physical VRAM or frame peaks.
Packed v12 explicitly refuses overflow tokens; uniform-only data fits its
existing block. Full versioned portable transport remains pending 78-R8.

At this instance-only milestone, structural changes still cleared unrelated
native programs; the next milestone below addresses that cache invalidation.
Full pre-placement effect IR and grouping, bounded parallel CPU
preparation, runtime-plan consumer negotiation, all-precision bound instances,
end-to-end costs/peaks, final integration and cross-platform CI remain required.
No #78 PR or issue-closure claim is made by these local targeted gates.

## Structural program reuse across registry replacement

The native renderer now retains successful pipeline proof independently of
registry-local program IDs. Active entries keep a private compiled-shader Arc;
healthy frames compare its identity. A new Arc is compared by complete actual
WGSL, sprite vertex layout and additive blend specialization. The target
scratch format is part of the key, and each cache belongs to one GPU owner.
Fixed SDK bindings, entry points and the other fixed native pipeline states
remain prerequisites. Package hashes and author IDs do not authorize reuse.

Registry replacement retires the last prepared generation into at most 128
pipeline entries and 8 MiB of WGSL comparison text. Empty intervening generations
do not erase that proof. Replacing a prepared generation replaces its retired
cache; this is not an unbounded history. It retains no packages, textures,
bind groups or admission leases. Resource invalidation and original package,
disabled-state, layout, capability and allocation admission still run. Evicted
code takes the ordinary compilation path. Current-generation entries retain
their existing working-set policy; the retired limits are not a new global
limit on live programs or on driver pipeline memory.

Successful portable fusion code also survives registry resets. Its program ID
becomes unbound and is assigned again on first use. Private successful code,
rather than a mutable public program copy, authorizes this rebind. The existing
64-group limit remains; at most 8 MiB of WGSL/GLSL/reflection text is retained as
inactive groups. Inactive entries yield capacity to new live groups. Failed
compilations remain suppressed within a generation and are reconsidered only
after explicit registry invalidation. Explicit portable/native fault injection
bypasses success proofs and reaches the real parser or device validation.

`Renderer::effect_pipeline_cache_info` publishes current/retired entry gauges,
retired WGSL bytes, limits and lifetime successful reuses. Fusion reports publish
portable rebinds, inactive entries and retained code bytes. These are cache
entries and retained UTF-8 bytes, not unique GPU objects, heap/VRAM peaks or GPU
completion counts. Cached portable metrics describe the original successful
compilation; rebinds are counted separately. Healthy cache misses and eviction
do not produce fault logs or change rendering policy.

| Source-bound receipt | Actual result |
| --- | --- |
| `issue78-numeric-structural-v1` | Desktop check and 226 render library tests passed; related render tests had 13 passes and one superseded assertion requiring redundant compilation after identical registry replacement |
| `issue78-numeric-structural-v2` | Desktop check, render library 226, related render 14 and default-output two passed; desktop node scope had 218 passes and only the new four-precision fixture failed because it called the PNG API for a high-precision target |
| `issue78-numeric-structural-v3` | Desktop check and corrected structural fixture passed; all production source and every other test source exactly match v2, verified by SHA-256; the only changed file appends the new fixture and preserves all pre-existing numeric tests |

The original lifetime test's exact compile expectation is deliberately updated
from two compilations to one compilation plus one successful rebind. Its scratch
feedback, format and lifetime assertions remain intact. The registry test now
requires zero extra native builds for independently revalidated identical code;
disabled-package refusal, bypass pixels and recovery are still checked. These
changes establish the requested new performance contract, rather than relaxing
correctness assertions. No test is ignored or removed.

The new structural fixture applies two independent editable graphs through real
Session/package paths. Across Unorm8, Unorm16, Float16 and Float32, changing the
left graph builds exactly one native program; every right-side raw pixel byte
is unchanged. The complete result equals an independently built cold renderer,
and a later warm frame adds no builds. Readback preserves native 4/8/8/16-byte
pixels; the production PNG precision refusal remains intact. This matrix does
not extend the previous private numeric/overflow matrix beyond its documented
Unorm8 scope.

Another four-precision native matrix reparses successful packages independently,
performs multiple registry changes before preparation and confirms identical
pixels with zero new native or portable fusion compilations. Unit tests cover
private proof integrity, inactive capacity eviction, metadata byte/entry limits
and exact WGSL/vertex/blend/format refusal. Capacity fixtures alias an actual
successful native handle; they do not claim to compile oversized shader text.

Actual cold and warm portable-parser/native-validation failures reach the
production sink at default Info: one Error in file and stderr, no diagnostic on
stdout, no logger call under the Session guard, stable primary causes on repeated
calls, and real recovery pixels. Their four child receipts and failure logs are
retained. The combined accepted scopes contain 461 distinct passing tests, zero
ignored; filtered empty targets and child-process counts are not extra passes.
This is targeted acceptance, not the full workspace or cross-platform gate.

The [verified structural archive](data/typed-render-dag-structural-385b530b.zip)
contains 223 files plus its manifest, 2,558,204 bytes, SHA-256
`6251e598ed5e56d2326a750d6bf403e1e0dfd0ea3e32e2f9acb7e3ed44e050c6`.
Every archived file and final Rust/Cargo source hash was read back and checked.

Full pre-placement effect-level IR and grouping, versioned packaged execution
and portable overflow consumption, bounded parallel CPU preparation,
all-precision private instances, complete IR fault channels, end-to-end
costs/peaks, final integrated regression and cross-platform CI remain pending.
Issue #78 stays open, followed by #74 and #87 in the user-required order.

## Local logical dependency admission for grouping

This checkpoint separates `scratch::logical_graph` from physical placement.
The placement wrapper still consumes the same checked graph and retains the
same storage-class, inclusive lifetime and terminal-copy rules. Opt-in
pointwise grouping now builds logical values before rewriting passes, then
requires every removed intermediate value to have no reader outside the group.
Terminal snapshot/composition consumers and preserved attachments remain
boundaries. A clear of the same semantic slot creates a distinct value; two
ports of one operation reading a value are not two independent consumers.

The dependency summary is bounded by the existing native pass/resource limit
and discarded after planning. Healthy fusion-disabled and cached-preview
paths add no such summary. Enabled uncached planning adds a logical graph
construction and use summary; no CPU or frame-time improvement is claimed.
Shader equivalence, quantization, alpha, external layer readers and compositing
proofs remain independently required. A closed dependency region alone is
never a proof that shaders can be fused.

Three new graph tests cover later source-port readers, repeated slot writes,
duplicate input ports, terminal snapshots, preserved attachments and invalid
group ranges. A fourth test uses real built-in stages and the actual portable
compiler: an invalid preceding raster import must prevent compiling an
otherwise admissible replacement, preserve existing placement metadata and
recover when rebuilt from healthy authoring input.

Source-bound validation `issue78-numeric-logical-grouping-v1` passed desktop
check, 230 render library tests and 14 related native tests across four test
targets, with zero ignored tests. The native scope retains four-precision
fusion/raw-pixel comparisons, extreme-alpha guards, registry replacement,
actual shared-diamond encoding and typed snapshot errors/recovery. Final
Rust/Cargo hashes match the accepted source snapshot and all four edited Rust
files pass the repository's edition-2024 formatting check. No panels changed.

The [verified checkpoint archive](data/typed-render-dag-logical-grouping-be0c4b38.zip)
retains 63 files plus its manifest, including source snapshots, the sequential
controller, test logs and native metrics. Size: 290,638 bytes; SHA-256:
`8201ec93ff8456ca482b4ed4d02d54e090dd5cf68deea8550330f059ea826d7d`.
Every archived file was read back and checked; all 49 accepted source hashes
were verified against the working tree. These are targeted gates, not final
workspace or cross-platform acceptance.

This remains a pass-draft def/use checkpoint. It is not complete effect-level
IR lowering: transactional preview admission still proposes physical layouts
while constructing individual effects, and final snapshot scheduling still
uses the existing atomic-draw IR. Full semantic front-end ownership, packaged
IR consumers, bounded parallel preparation, four-precision private instances,
new IR failures through production output channels and final integrated CI
remain pending. Issue #78 is not ready to close.


## Effect-level image IR and actual native tasks

The current native consumer lowers one logical source import per draw and one
operation per accepted effect or host materialization. Layer and effect IDs,
canonical color/alpha/precision, integer texel grids, exact output view regions,
input roles, code identities, costs and conservative time/halo declarations are
separate from physical texture identities. A source import is a requested
canonical logical view, not an assertion about the original asset's stored
precision or physical extent. Its raster/conversion work is charged to the
native task that materializes it. Physical pass dimensions and allocations
retain their independent device and budget checks.

The graph is checked before fusion and final physical layout. Actual fusion
admission reads its primary chain and unique consuming operations, as well as
local SSA def/use closure and the existing implementation-bound shader proofs.
Raw layer readers attach to the source import; effected readers attach to the
final operation. Matte and explicit materialization requests retain terminal
snapshot roots. Blur/warp, format/ROI changes, external producers, alpha,
quantization and compositing boundaries keep their original proof requirements.

Fusion records a complete operational mapping from original drafts to current
native pass ranges. It does not read the 128-entry diagnostic report to execute
groups. Overlapping ranges form tasks with all member node IDs, including any
initial raster prefix. Checked graph order determines task completion order;
complete native pass coverage must agree with each atomic draw's admitted
scratch lifetime order. The renderer consumes these task/pass orders and the
graph's selected snapshot order, with the same GPU owner and command encoder.
The earlier coarse snapshot API remains available for diagnostic inspection.

Code identity work is bounded by a 1024-entry cache of weak shader references.
Actual WGSL/GLSL, reflection names/bindings, sprite/blend flags and loop metrics
form the digest. Sampled numbers and origins do not. Public shader Arc mutation
or replacement invalidates a hit; registry resets clear entries. Hash and text
byte counters accumulate for the PlanBuilder lifetime. Weak entries do not keep
shader payloads or GPU resources alive. `hashed_text_bytes` counts UTF-8 source
and reflection names, not hash framing or binary binding words. Logical costs
retain unfused work; current native task ranges describe lowered pass counts.
Neither is a driver-memory or measured frame-time claim.

New acceptance covers operation/task ownership, numeric hash reuse, a real code
change affecting one digest, bounded weak ownership, canonical boundary refusal,
cycle origins/recovery, capability/type/budget negotiation, virtual source views
and early bounded origin rejection. The virtual-view test first failed against
the device rectangle limit, then passed with the logical/physical limits split;
a genuinely oversized materialized pass must still fail. The origin test first
failed because code preparation preceded rejection; it now requires rejection
before new hashing, bounded single-line context and healthy recovery.

Retained attempts: v1 passed desktop check and five new tests; the new cycle
fixture's transparent-error traversal was corrected without weakening its cycle
or recovery assertions. v2 passed check, focused six and render library 236,
then all-native compilation exposed one old allocation-test initializer missing
`instance_data`; its value is now explicitly zero. The boundary-red run passed
six tests and failed both new regressions for the intended reasons. Final v3
acceptance kept all 52 Rust/Cargo source hashes unchanged throughout:

| Gate | Actual final result |
| --- | --- |
| Desktop check with FFmpeg | Passed |
| Focused effect IR | 8 passed; a subset of the render library below |
| Complete render library | 238 passed, zero failed/ignored |
| All 101 render integration targets | 418 passed, zero failed, one existing optional probe ignored |
| Default compiler failure output | 2 parent tests passed, zero failed/ignored; four cold/warm portable/native child cases |
| Desktop node scope | 219 passed, zero failed/ignored, including one integration test |

There are 877 distinct passed tests in these final scopes; the focused eight
and passes from earlier attempts are not added again. The existing ignored
`optional_local_original_image_probe` needs an external `MOTION_IMAGE_SOURCE`.
No new skip or weaker assertion was introduced. This is not a final full
workspace or cross-platform CI result.

The four actual compiler child cases retain default Info starts, exactly one
Error in the file and matching stderr output, without compiler diagnostics on
stdout. Repeated failures do not retry compilation; healthy recovery renders
real pixels. These retain existing compiler fault coverage, not complete new
IR cycle/type/budget failures through production output channels.

The [verified effect IR checkpoint](data/typed-render-dag-effect-ir-714f4668.zip)
retains 302 files plus its manifest, including all four attempts, source
snapshots, controllers, native outputs and default-channel logs. Size:
2,932,635 bytes; SHA-256:
`05398722d0dd76c3649ff908500ce9fed1a0b5b3857e519510a99ee7f7841c60`.
Every entry was read back and verified; accepted hashes still match all 52
source files. The two new modules, pointwise planner and corrected allocation
fixture pass edition-2024 formatting checks. No panels changed.

This is an effect-level image consumer built from accepted native drafts. CPU
generators, vector/mask preparation and transactional preview layout proposals
still precede this lowering. Package image/LUT/mask/ShaderData imports retain
native admission rather than becoming fully typed generic producer nodes.
Complete initial semantic ownership and portable runtime template consumption
remain unfinished. This checkpoint does not establish parallel CPU preparation,
all-precision private numeric instances, complete IR failures through production
output channels, final integrated regression or cross-platform CI. Issue #78
remains open, followed by #74 and #87.


## Persisted package image execution and real native branch scheduling

The [versioned image-template contract](../host-api/runtime-execution-template.md)
is now emitted by actual graph export, validated by the real `.msfx` loader and
instantiated by the native planner. Manifest and graph capabilities, exact
version, complete pass ownership, typed dependencies, terminal closure and
actual compiled-code identities are checked. Current origin, channel precision
and input/output extents are bound at instantiation rather than persisted as
texture handles or sampled values. Ordered package resources retain their
existing native admission; they are not yet fully typed external producers.

Shared code-identity construction moved into `motion-effects::shader`; native
weak-cache ownership, actual code/reflection inputs and lifetime work counters
retain their previous semantics. Numeric data and origins do not enter the
digest. The digest is neither a publisher signature nor an equivalence proof.

The actual native consumer uses checked topology and lifetimes before proposing
physical working storage. Both sampled image ports follow declared edges; the
SDK clock keeps each original shader-pass index. Three existing working images
remain the current capacity. Sampled inputs cannot be attachment outputs on the
same pass, even at their final use. Exceeding that capacity returns a typed
`TemplateError::Storage` with the refusing node and required/available counts.

The real four-precision package test deliberately stores shaders as B/D/A/C and
template nodes out of order. Actual execution is A/B/C/D, with one preparation
of shared A, seven native passes and eight draw calls including composition.
Raw typed pixels agree with an independently authored storage-aware numeric
reference. The control that removes the execution template produces different
pixels; the test would fail if the consumer merely ignored the template.
Warm rendering adds zero native pipeline builds. This image DAG matrix is
separate from the actual bound-instance matrix described below.

The new storage-refusal regression first failed in retained v2: the old preview
policy dropped the failed effect and returned `Ok` with one plain draw. The
implementation now returns template/structural execution errors even during a
non-strict preview. This is an explicit functional refusal rule, separate from
observability. Ordinary effects retain their existing preview error policy.
The unchanged test now verifies the original typed cause, layer/effect context,
no new pipeline builds, preservation of prior GPU pixels and healthy recovery.
Actual ZIP loader tests additionally cover version/capability refusal before
shader parsing, changed compiled code, typed cycles, contract faults and recovery.

Retained v1 passed only desktop check and four focused tests. Retained v2 passed
effects 100, graph 80 and render library 238, then exposed the real refusal bug.
Retained v3 then passed all lower-layer gates and found an existing desktop
contract violation: grouping changed the newly visible code identity in the
manifest. Its 217 successful desktop unit tests and one failure are retained.
A new exact-code regression failed in v4. Iterative emission now follows
output dependencies in input-port order rather than authoring node IDs, and
generated temporary source omits authoring-ID comments. Authoring diagnostics
and reports retain their IDs; explicit preview source retains its inspected
port. Validation still rejects cycles/types/budgets before emission. Four-level
wrapping now preserves actual source, WGSL, GLSL and complete template manifests
in both baked/editable compilation, without weakening the old equality test.
Retained v5 proved that focused correction.

A separate bound-instance test uses four native precisions, uniform/overflow
carriers and two numeric updates each: 16 actual raw-pixel comparisons against
both independent analytic values and separately baked GPU implementations.
Eight precision/carrier cases additionally save and reopen the actual project
from disk, restore its embedded package without recompiling the authoring graph, verify two
independent instances and restore original pixels by undo. Package Arcs and
native pipelines are retained across numeric updates; unchanged values cause
zero overflow upload bytes. Raw 16/32-bit images are never reduced to PNG for
these comparisons. These are real maintained-desktop consumers. All Android work is excluded by
explicit user instruction; no Android execution or device result is claimed.

Earlier focused/subset results are not added to final counts. Final source-bound runtime gate:

| Gate | Actual final result |
| --- | --- |
| Desktop check with FFmpeg | Passed |
| Focused template unit scope | 4 passed, zero failed/ignored; 1 result targets |
| Complete effects crate | 100 passed, zero failed/ignored; 18 result targets |
| Complete graph crate | 81 passed, zero failed/ignored; 20 result targets |
| Complete render library | 238 passed, zero failed/ignored; 1 result targets |
| Template, fusion and typed-snapshot native targets | 14 passed, zero failed/ignored; 3 result targets |
| Desktop node scope | 220 passed, zero failed/ignored; 6 result targets |

There are 653 distinct passed tests in these selected final
scopes. The focused four are already included in effects-all. No assertion was
weakened and no test was newly skipped. These are targeted gates, not the final
full workspace or cross-platform CI result. All 62 changed
Rust/Cargo source hashes stayed unchanged throughout and still match the accepted
snapshot. Selected changed Rust modules/tests pass edition-2024 formatting checks.
No panel files, UI, dependencies or third-party assets changed.

The [verified runtime checkpoint](data/typed-render-dag-runtime-fe523a2b.zip)
retains all attempts, including actual red refusal and grouping cases, sources,
controllers, test logs and raw numeric metrics. Size: 2,325,955 bytes;
548 files plus its manifest. SHA-256:
`3c049b68ebddf7c03a09d4f6b3f51703c2e7f29e22b21c9446d3805e5816ec31`.
Every archive entry was read back and verified against its manifest.

Packed v12 remains the frozen Android/GLES adapter and explicitly refuses
private instance overflow. The user excluded all Android implementation and
acceptance on 2026-10-25 because Android is no longer maintained; extending that
adapter is not a desktop completion blocker. Complete initial desktop semantic
ownership before CPU preparation and transactional preview layouts, fully typed
external imports, bounded parallel CPU preparation, measured end-to-end costs,
complete IR faults through production output channels and final integrated/cross-platform acceptance remain pending.
Issue #78 remains open, followed by #74 and #87.


## F: real execution-IR failures after aligning with main

Before this checkpoint closed, `git fetch origin main` and `git rebase origin/main`
aligned all nine owned commits with main `4afb6cfc036393d3ec02062ae7a846a26403b1df`. The rebased runtime
checkpoint is `6adde1d60792a2a1d3d9eceb2db800c77567e944`. `range-diff` reported unchanged patches for all
nine commits. All eight uncommitted files were backed up, restored and verified
with normalized source hashes. The exact pre-rebase commit/stash and patch mapping
are retained with the evidence. Earlier receipts remain historical checkpoints;
they do not substitute for testing the current integrated source.

The new desktop test runs real production `ExecutionPlan::check` failures through
the actual owning Engine worker, real Session guard, Renderer, command error
reply and default file logger. No `MOTION_LOG` override is set. A diagnostic-only
one-shot injector changes the actual draft dependency, expected input precision
or pass limit immediately before the real checker. It neither manufactures an
error result nor bypasses validation. Injection and its selector are absent from
release builds. The existing test-only worker function gateway is now available
on all desktop test platforms rather than only Windows/FFmpeg; it remains absent
from the application binary. Production `respond` and best-effort reply semantics
are unchanged.

Two failure/recovery episodes each cover cycles, type mismatches and pass-budget
refusal. Every typed cause is checked before conversion to a reply. File and Error
stderr each contain six actual failure records, including request, operation,
elapsed time, composition, layer 47, effect 11 and original check details. The
matching non-Present Info start precedes every failure. Info starts appear only
in the file; stdout contains none of these diagnostic records. The parent test
captures and inspects the real child process's two output channels.

All six refusals occur before new native pipeline construction and preserve the
already rendered target pixels. Healthy frames use the same Renderer and target,
match the original raw bytes and recover without rebuilding pipelines or changing
the worker's project. No retry/restart policy or new per-command timeout is added.
Healthy Present behavior and UI presentation are unchanged.

Retained v1 failed at compilation because diagnostic helper visibility was too
narrow. Retained v2 completed all six fault/pixel/recovery checks but its child
hung during teardown: a test-owned GPU target remained in TLS until worker thread
exit. It is a failed attempt, not a pass. Explicit owner-thread fixture cleanup
now releases the target before the Renderer and TLS destruction. Retained v3
passed all targeted gates before rebase. This final post-rebase gate reran them
against the aligned source, without changing existing assertions or adding skips.

| Gate | Actual post-rebase result |
| --- | --- |
| `desktop-check` | Passed |
| `ir-default-output` | 1 passed, zero failed/ignored |
| `render-no-diagnostics-check` | Passed |
| `render-ir` | 8 passed, zero failed/ignored |
| `existing-compiler-output` | 2 passed, zero failed/ignored |
| `existing-worker-api` | 1 passed, zero failed/ignored |

The selected scopes contain 12 distinct parent tests; the
child's six negative/recovery cases are evidence, not six additional test counts.
All 119 Rust/Cargo snapshot hashes stayed unchanged across
the gates and match the archived source. The no-default-feature render check
also verifies the build without the diagnostic interface. Selected new/changed
modules pass edition-2024 formatting. No owned panel, UI, dependency, asset or
Android change is included. This is F's targeted checkpoint, not G's complete
workspace regression or cross-platform desktop CI result.

The [verified IR output checkpoint](data/typed-render-dag-ir-output-6adde1d6.zip) retains all attempts,
actual file/stderr/stdout logs, adapter metadata, raw baseline pixels, recovery
receipts, source snapshots, controllers and rebase proof. Size: 6,509,103
bytes; 497 files plus its manifest. SHA-256:
`a730fd9a0bc9798c7abb01c40d892b407076c6064ee76ac9fb17318d22b5780d`. All entries were read back and matched their manifest hashes.

The maintained desktop still needs complete initial semantic ownership before
CPU preparation/transactional preview layouts, fully typed external producers,
bounded parallel CPU preparation, measured end-to-end costs and final integrated
regression/cross-platform CI. G must run on the aligned final source. All Android
implementation and acceptance remain excluded by explicit user instruction.
Issue #78 stays open; the following order remains #74 then #87.


## Bounded parallel vector CPU preparation after the main rebase

Pure vector tessellation is now separated from the native effect/draw loop.
Per-layer sampling density is calculated once and consumed by both preparation
and effect planning. At most four jobs execute in a scoped batch, including the
calling thread; the default limit is capped by available parallelism. Only pure
CPU geometry runs on these threads. Device, queue, command encoder, physical
placement and GPU submission keep their existing owner.

Each job uses the batch's remaining vertex limit, which is never greater than
262,144. Both pending jobs and completed results are bounded before cache
admission. The owner drains prepared geometry and cache hits in scene order and
charges the existing frame-wide vertex limit before inserting a new mesh. A
rejected result and later speculative results never enter the cache. Earlier
valid meshes remain admitted when a later geometry or setup operation fails.
All workers are joined before resuming an original panic payload; panics are not
converted into successful geometry. Worker launch failures retain their IO cause
and frame context. No retry, worker restart or hot-path file logging was added.

`PlanBuilder::cpu_preparation_info()` reports per-build job/cache counts, the
largest batch and logical pending vertex bytes. The last field explicitly
excludes cached geometry, Lyon temporary buffers, allocator overhead and GPU
memory; it is not a process or driver peak measurement. Cache-only builds launch
no workers and reuse the existing vertex Arcs. Mask and scene-generator state
remain serial. Moving vector preparation ahead of effect lowering can change
which malformed operation fails first across different preparation categories;
valid composition order and per-vector cache/budget admission stay unchanged.

New controlled tests prove actual overlapping entry before release, ordered
results, a four-job cap, joining on panic with the original payload, exact
serial/parallel geometry/paint/fingerprint equivalence, zero warm tessellations,
ordered global-budget refusal without speculative cache insertion, and recovery
from real setup/geometry failures. Existing assertions were not modified.
Retained v1 failed compilation because viewport dimensions still used by later
budget checks were accidentally removed during extraction. Restoring those
variables produced the accepted source; the failed attempt remains archived.

| Targeted gate | Actual result |
| --- | --- |
| `render-cpu` | 5 passed; zero failed/ignored |
| `render-vector-budget` | 1 passed; zero failed/ignored |
| `desktop-check` | Passed |
| `render-library` | 243 passed; zero failed/ignored |
| `render-native` | 36 passed; zero failed/ignored |

There are 279 distinct tests in the complete render-library
and selected native scopes. The first six focused tests are also in the library
scope and are not counted twice. All 121 Rust/Cargo hashes
stayed unchanged during the gates. The native scope covers prepared resources,
preview memory, runtime execution templates, typed snapshots, vector adjustment,
budget alignment, actual error boundaries, groups and upload reuse. The desktop
FFmpeg check also passed. No panels, styles, UI behavior, dependencies, assets or
Android source were changed.

The [verified preparation checkpoint](data/typed-render-dag-cpu-prepare-50291554.zip) retains
both attempts, actual stdout/stderr, source snapshots, controllers and the patch.
Size: 1,355,299 bytes; 265 files plus manifest. SHA-256:
`f8d70f39288bb323daa5ba9886f5436d475b1025e8d1fde00f390c4c37475cfb`. Every entry was read back and checked against its hash.

This is a targeted checkpoint, not G. Initial semantic ownership before all CPU
work, typed external imports, full end-to-end shader/compile/pass/frame/physical
peak measurements and final integrated cross-platform desktop CI remain open.
All Android implementation and acceptance remain excluded.
