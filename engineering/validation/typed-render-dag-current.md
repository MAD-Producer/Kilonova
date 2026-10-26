# Typed render DAG: current delivery ledger

Issue #78 is **in progress**, followed by #74 and #87. This is a partial
implementation record, not an issue-closure claim. The initial foundation was
based on `40efcd35`, then integrated with #73 at `ebb983c0`; the foundation
commit is `6fdc5f1d`; the native snapshot milestone is `7fe37354`. PR #491
passed all required checks and merged as `7bd4d860`, with its actual merge
subject/body verified in Chinese and issue #73 closed. No other agent's PR diff
was reviewed and no subagent was used.

## Requirements and actual coverage

Latest closure audit: C1 and C3 are implemented with the source-bound targeted
receipts below. Five independent fusion obligations are being accepted with
their own arguments and controls. Final latest-main whole-workspace regression
and maintained desktop CI remain required; earlier passing checkpoints are not
attributed to later source.

| Item | Current evidence | Remaining requirement |
| --- | --- | --- |
| 78-R1 authoring / execution / frame ownership | Actual model relationships lower to snapshot IR; exported graph packages now carry a reusable image execution template, checked by the real loader and instantiated by the native consumer | Pre-raster semantic producers, sampled selectors, origin IDs and region recipes now drive native proposals; admission refines real texel grids and aliases preview bypasses; final main-aligned integrated acceptance remains open |
| 78-R2 typed values and semantics | Versioned image values, origin IDs, distinct input roles, canonical color/alpha/precision, coordinates, rectangles, depth-direction rules, time, halo and state/capabilities | Package PNG/encoded defaults, current-instance byte containers, actual numeric curve LUTs and scalar mask coverage now supply checked values consumed by native preparation; depth producers belong to later work |
| 78-R3 checked topology and lifetime | Checked effect-level operations control actual snapshot/native task coverage; checked package-local DAG order and lifetimes control native pass order, both sampled image ports and working-slot placement | Early effected-source topology refuses cycles before CPU work; the initial semantic graph now owns outer producer identity and dependency selectors before proposals, while concrete checked texel extents follow transactional admission |
| 78-R4 actual shared upstream work | Four-precision native snapshot traces retain one encode/copy per shared producer; four-precision package-local diamonds execute A/B/C/D despite a B/D/A/C shader list, with actual draw counts, an independent numeric oracle and a differing linear-list control | Final integrated boundary acceptance on the current main baseline |
| 78-R5 numeric data / code identity | Numeric updates preserve validated packages and native pipelines; four-precision uniform/overflow instance tests verify 16 raw-pixel updates and eight disk reopen/isolation/undo cases; structural edits rebuild only the affected program; grouping preserves actual generated code and runtime manifests | Real two-group portable/native counters now verify zero numeric compilation and only affected structural compilation; CurrentSample tasks still execute per frame; cache eviction may require recompilation; final integrated CI remains |
| 78-R6 proven grouping | #73 implementation-bound proof, local SSA def/use closure and effect-level IR primary-chain/unique-consumer checks jointly gate actual groups before final physical placement | Complete semantic import/boundary coverage and final acceptance; closed graph regions alone never establish shader equivalence |
| 78-R7 CPU / GPU scheduling | Pure vector tessellation now runs in bounded batches of at most four jobs with ordered global-budget/cache admission; native GPU ownership, encoder and atomic scratch allocator stay unchanged | Mask/generator state remains serial with its existing mutable cache/budget ordering; bounded pure-vector CPU parallelism fulfills the current scope; final integrated scheduling acceptance remains |
| 78-R8 runtime version / capabilities | Version-1 package image templates require image_execution_v1, bind actual compiled code and current precision/extents, and execute natively; actual loader tests reject versions/capabilities/code/cycles and recover; the targeted source-bound runtime gate passed | Complete final desktop integration/CI; extending the retired Android/GLES packed ABI is excluded by user instruction |
| 78-R9 failures / observation | Typed cycle, precision mismatch and pass-budget failures now traverse the actual worker and default file/Error stderr channels with request/origin/timing context, stdout isolation, original GPU pixels and same-renderer recovery; earlier compiler/private-data faults remain covered | Confirm the complete final source in G integration and cross-platform desktop CI |
| 78-R10 performance / admission | Physical scratch/source admission remains authoritative; a bounded weak code-identity cache avoids rehashing unchanged shader text on numeric rebuilds | Actual dev-profile shader sizes, portable/native compilation, pass counts, synchronized frame wall intervals and per-frame enrolled-capacity peaks are recorded below; no process RSS, driver VRAM or GPU-only timing inference; final integration remains |
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


## Initial sampled parameter and image-input ownership before CPU preparation

The planner now owns enabled effects per layer, validated numeric/uniform and
private-data-layout values, and distinct absent/empty/asset/raw-layer/effected-
layer image inputs before mask rasterization, vector tessellation, scene-generator
CPU work or transactional per-effect scratch proposals. Native lowering consumes
these prepared records once; it no longer re-derives the same sampled inputs from
integer resource tokens. ABI uniform flags and integer bindings are generated
from the typed source selection at the native boundary. Ordinary missing layer
inputs still mean empty input, and zero opacity still validates parameters before
becoming an identity. Scene operation counts and bounded composition identity are
validated before effect synchronization; stale checked receipts are cleared on
these early failures.

A bounded dependency projection checks same-frame effected-layer references
before expensive CPU preparation. It follows the actual primary effect chains,
distinguishes raw from effected sources, omits zero-opacity identities and
invalid candidates, and adds image-input edges only for native image consumers.
It reports an actual cycle without downstream tails, retaining the existing typed
cycle cause, bounded operation members and layer/effect/composition context. Raw
sources and chains without effected-source references avoid this second graph
walk. GPU owners, composition draw order and final checked native task ordering
are unchanged.

This deliberately changes structural refusal timing: a malformed cycle is now
an error before CPU work, including in legacy preview, even if a later physical
refusal might previously have bypassed one member. Parameter, shader-layout,
package and program errors remain owned results until the existing per-effect
refusal policy consumes them, preserving ordinary preview bypass and strict/
guarded refusal. The accepted-region calculation, downstream ROI and scratch
proposals still follow actual previous effect admission; no prospective output
rectangle is reused after a bypass. This is a functional early validation change,
not merely a logging edit.

The projection does not invent image rectangles, storage formats or precision
for preparation tasks. The final full typed image IR remains authoritative after
accepted native proposals. Complete producer/value/ROI ownership ahead of those
proposals and typed package PNG, curve LUT, mask coverage and encoded private
imports remain unfinished. In particular, data/LUT bytes are not relabeled as
working color to claim coverage.

Six new tests exercise real cyclic sampled input before deliberately malformed
vector geometry (zero geometry jobs/cache admission), recovery, exact native
input flags and snapshot selection for all input families, original ordinary
preview cause/bypass, zero-opacity/raw-input cycle exclusion, bounded early
origin refusal that clears a prior checked receipt, and exclusion of residual
downstream tails from cycle diagnostics. Existing assertions were not modified.
The initial focused five-test checkpoint and the final six-test source are both
retained; the companion precision gate uses the identical final source hashes.

| Targeted gate | Actual result |
| --- | --- |
| `render-preparation` | 6 passed; zero failed/ignored |
| `render-ir` | 8 passed; zero failed/ignored |
| `desktop-check` | Passed |
| `render-library` | 249 passed; zero failed/ignored |
| `render-native` | 69 passed; zero failed/ignored |
| `ir-default-output` | 1 passed; zero failed/ignored |
| `private-numeric` | 1 passed; zero failed/ignored |
| `private-precision` | 1 passed; zero failed/ignored |

These scopes contain 321 distinct parent tests. The focused
preparation and older IR tests also belong to the complete library scope and are
not counted twice. The companion private-instance test includes 16 real numeric
updates and eight reopen/independent-instance/undo cases across all four native
precisions, with raw pixel/analytic comparisons. These are cases within one test,
not extra parent-test counts. The existing real worker default-output gate also
passes on this source: six actual typed fault/recovery episodes, file and Error
stderr output, non-Present starts, stdout isolation, preserved pixels and no new
pipeline construction. Its diagnostic injection still targets the final image
IR; the new early-cycle cases are independently checked through the planner.

All 123 Rust/Cargo source hashes match across these gates.
No panels, styles, product interactions, dependencies, assets, or Android files
were changed. This is targeted current-source acceptance, not G's complete
workspace or cross-platform desktop CI.

The [verified initial preparation checkpoint](data/typed-render-dag-preparation-f8a889f3.zip)
retains all attempts, actual logs, source snapshots, controllers, default-fault
pixels/receipts, the four-precision receipt and patch. Size: 2,491,841
bytes; 419 files plus manifest. SHA-256:
`e1c43f7cc0500cba1f92fb7132ee5cfe0a1135d2474b605f0e2c9efacf4f03f6`. Every archive entry was read back and hash-verified.

Remaining work includes full semantic producer/value/ROI ownership and external
imports, measured end-to-end shader/compile/pass/frame/physical peak costs, and G
full integrated regression/cross-platform desktop CI on the aligned final tree.
Android implementation and acceptance remain excluded.


## Retained semantic region rules and parameter identity

Resolved effects now retain immutable output-region expressions, indexed
parameter identities, working-space/alpha semantics, edge selection and opacity
selection. Prepared effects bind frozen definition-ordered numeric values to
those rules. Actual native lowering consumes the prepared rules rather than
rescanning sampled authoring arrays for each field. Numeric rebuilds retain the
same per-resolved-effect Arc and native program/code identities. This is not a
global semantic cache or a complete initial producer/value graph.

Region expressions are evaluated against the actual previously admitted input.
If preview bypasses an oversized effect, the next effect uses the unchanged
input extent. Padding, explicit output rectangles, input origins/sizes, vector
parameter components and negative-padding errors preserve their existing
contracts. Region evaluation still occurs at transactional native admission;
this milestone does not claim complete pre-placement ROI ownership.

Separately, this fixes an existing functional bug: edge selection looked up an
index in sorted sampled parameter IDs, then used it to index the definition's
declared parameter order. For glow-like definitions this interpreted Enum
Repeat/Mirror as a Boolean and selected Clamp. Edge selection now uses parameter
identity and definition order throughout. Restoring only the legacy resolver
made the unchanged new uniform regression fail with Mirror 3 becoming Clamp 1.
It also made the unchanged actual GPU pixel oracle fail for Repeat with an
error of 219 sRGB storage codes. Green source and fixture hashes were verified
after restoration.

Six new library tests cover actual native flags/opacity/regions, reuse on numeric
rebuild, frozen values, accepted-input refinement after preview bypass, exact
negative-padding cause and explicit rectangle/vector-component evaluation.
An additional real GPU integration test uses independently authored ramp/offset
shaders and an integer-coordinate oracle for Clamp/Repeat/Mirror. All pixels
are checked within one sRGB storage code; decoded linear error is also retained.
All three modes use eight actual draw calls and the same four native pipeline
builds. Strict preflight rejects an invalid edge value without replacing prior
target pixels, and the same renderer recovers the original Clamp bytes without
recompilation. Interactive per-effect bypass remains unchanged.

Retained failed fixture attempts are not acceptance: the first library attempt
expected an unprefixed Display string; pixel v1 used a renderer format that did
not match the typed sRGB target; v2 confused linear-light units with sRGB storage
codes; v3 expected interactive draw to perform strict preflight. Only newly
authored fixtures were corrected. No previously committed assertion, skip,
production refusal policy or pixel bound was changed to pass these gates.

| Targeted gate | Actual result |
| --- | --- |
| `render-semantics` | 12 passed; zero failed/ignored |
| `render-ir` | 8 passed; zero failed/ignored |
| `desktop-check` | Passed |
| `render-library` | 255 passed; zero failed/ignored |
| `render-native` | 98 passed; zero failed/ignored |
| `ir-default-output` | 1 passed; zero failed/ignored |
| `private-numeric` | 1 passed; zero failed/ignored |
| `private-precision` | 1 passed; zero failed/ignored |
| `edge-pixels` | 1 passed; zero failed/ignored |

These scopes contain 357 distinct parent tests. The focused
semantic/preparation and earlier IR tests are subsets of the complete library
scope and are not counted twice. The private four-precision companion includes
16 numeric updates and eight reopen/instance-isolation/undo cases. The default
worker gate retains six real typed fault/recovery episodes, file and Error
stderr output at the default level, stdout isolation, preserved pixels and no
new native pipelines.

The broad gate froze 125 Rust/Cargo source hashes.
The companion freezes 126: every earlier hash is identical,
and the only added source is `crates/motion-render/tests/semantic_edge_modes.rs`,
which the companion compiles and runs. The archive verifies this exact set
difference; it does not ignore arbitrary test/source changes. No panels, styles,
UI interactions, dependencies, assets or Android files were changed.

The [verified semantic-region checkpoint](data/typed-render-dag-semantic-regions-39e17aa3.zip)
retains every failed/successful attempt, source snapshots, actual logs, GPU
receipts, controllers, legacy restoration proofs and the patch. Size:
12,314,938 bytes; 1223 files plus manifest. SHA-256:
`ed06d849714c91202225f82d511fbefb8dc3621fd0c0e8c91abc9f1b04cee25c`. Every archive entry was read back and hash-verified.

This is targeted checkpoint acceptance, not G or issue closure. Full semantic
producer/value ownership, typed package PNG/LUT/mask/private imports, affected-
group scheduling, end-to-end shader/compile/pass/frame/physical-peak measurements
and final main-aligned integrated regression/cross-platform desktop CI remain
open. Mask/generator preparation remains serial. Android implementation and
acceptance remain excluded.


## Checked package images and encoded numeric defaults

Package resolution now retains image-resource dimensions, numeric meaning,
resource-local coordinates, storage and explicit byte encoding before vector,
mask or generator CPU preparation. Normal PNG channels are Data Vector4 /
UNORM8 / Rgba8Unorm. A private numeric default uses that same byte-container
image contract plus its LittleEndianF32 scalar-count declaration. Its shader
explicitly reconstructs f32 bits; the image is not relabeled as Float32 color.
Neither import silently acquires the project's working color space or alpha.

The execution graph owns real source values and typed resource edges for actual
package bindings. Repeated passes/effects share a source value with two real
logical consumers in the new regression. Dynamic image/private overrides are
alternatives to those package slots, not additional samples of package defaults.
Source values add no fake draw passes. Native preparation requires a matching
checked value before using package bindings, uses retained dimensions for pending
admission, verifies decoded extent/storage, and allocates the declared format.
No PNG header scan or file read was added to healthy per-frame polling.

Three new library tests verify shared producer/lifetime edges, exact numeric and
byte-container semantics, binding/format/extent refusal and recovery, and the
original typed image-header cause with bounded package/path/phase context.
The independently authored GPU fixture samples an asymmetric 3x2 numeric PNG or
decodes a private f32 byte container, with two effects and all four working
precisions. All eight cases match their independent reference and retain native
pipelines on numeric updates. Source bytes remain UNORM8 for every working mode.
An additional native companion verifies one actual application texture identity,
descriptor and capacity per resource, and retention of that identity on numeric
updates. These are application-owned allocation descriptors, not driver
residency or physical-memory peak measurements.

The first focused attempt failed the existing resource-index admission guard:
the proposed port namespace started at 16 while the maximum resource index was
15. It was corrected to the actual alternative group-2 slots 0..3. The limit and
all existing assertions were retained. The earlier two-test focused source is
historical; the final library scope includes the third header-cause test.

| Targeted gate | Actual result |
| --- | --- |
| `render-imports` | 3 passed; zero failed/ignored |
| `import-pixels` | 1 passed; zero failed/ignored |
| `effects-library` | 32 passed; zero failed/ignored |
| `desktop-check` | Passed |
| `render-library` | 258 passed; zero failed/ignored |
| `render-native` | 99 passed; zero failed/ignored |
| `host-memory-policy` | 17 passed; zero failed/ignored |
| `ir-default-output` | 1 passed; zero failed/ignored |
| `private-numeric` | 1 passed; zero failed/ignored |
| `private-precision` | 1 passed; zero failed/ignored |
| `native-allocation-identity` | 1 passed; zero failed/ignored |

These scopes contain 411 distinct parent tests. The focused
three-test library filter is included in the complete render library and is not
counted twice. The default worker gate still verifies six real typed failures,
default file/Error-stderr output, stdout isolation, preserved pixels and recovery.
The private-instance companion still verifies 16 numeric updates and eight
reopen/isolation/undo cases across four precisions. Their existing assertions
were unchanged; that regression does not claim new typed dynamic-instance edges.

The broad gate froze 132 Rust/Cargo hashes. The final
native-allocation companion freezes 133; every earlier
hash is identical and its sole added source is the allocation-identity test.
The archive verifies this exact addition, every retained source/log hash and
the actual native receipts. No panels, styles, product interactions, dependencies,
third-party assets or Android source were changed. The host's manual program
fixture only supplies the new empty import field; none of its assertions changed.

The [verified package-import checkpoint](data/typed-render-dag-package-imports-f06d7c79.zip)
retains all attempts, actual logs, source snapshots, GPU receipts, controllers and
the patch. Size: 3,264,220 bytes; 598 files plus manifest.
SHA-256: `d0165b1667a7b7605cc169c95317556a08e1fa894a6e29b94c056ab4c5db0686`. Every archive entry was read back and hash-verified.

This is targeted checkpoint acceptance. Full producer/value/ROI ownership before
placement, dynamic instance-data/LUT/mask imports, affected-group scheduling,
end-to-end shader/compile/pass/frame/admitted-peak measurements and G's final
main-aligned integrated regression/cross-platform desktop CI remain unfinished.
Mask/generator preparation remains serial. Android implementation and acceptance
remain excluded. #78 is still open, followed by #74 and #87.


## Current-sample private data, curve LUTs and mask coverage

Checked source values now describe actual one-based private instance sidecars,
numeric curve tables and layer-local scalar mask coverage. Explicit imports add
typed resource edges to their real consumers and no fictitious native draws.
Private instance data stays an RGBA8 little-endian f32 byte container, distinct
from floating-point color. Its sources are distinct per instance; native cache
ownership is unchanged. Package slots 1..3 remain dependencies when a private
instance replaces slot 0. The previous package-import lowerer incorrectly omitted
those companion edges; this checkpoint corrects that logical omission while
preserving the actual shader binding behavior.

UNORM8 curves retain 256x1 Rgba8Unorm numeric data. The other three working
precisions retain authored dense Float32 numeric tables, sampled without a
filtering/blending requirement. Actual native LUT preparation consumes the
checked width/storage and validates retained textures before cache shortcuts.
Unreferenced scene tables remain prepared as before. Negative LUT tokens retain
their existing system-identity behavior. Missing high-precision authored sources
are rejected before native allocation with the original diagnostic text.

Masks retain Coverage meaning, layer-local coordinates, actual raster extent
and R8Unorm/R16Unorm/R16Float/R32Float storage. Native output preparation uses
the checked descriptor and validates actual retained texture dimensions/format
before fingerprint reuse. Frames without effect passes still construct the
same typed coverage descriptor for their final compositor; no effect graph or
new shader work is fabricated for that path. Raster/filter preparation remains
serial and existing transactional scratch/output admission is retained.

Three new CPU tests verify distinct instance sources/shared companion edges,
all-precision LUT/coverage semantics and missing-source refusal/recovery. A new
native test rejects stale precision and a wrong-format texture behind an
unchanged fingerprint, preserves output bytes/owner and recovers. Two new GPU
tests cover 16 private numeric updates and 12 curve/mask updates across four
precisions, independent pixel references, real texture IDs/formats/capacities,
pipeline reuse and same-renderer refusal/recovery. Interior/exterior mask pixels
use an analytic reference; edge rasterization is not represented as independently
proved. Allocation IDs describe application-owned backing, not driver residency.

Two unchanged-test red controls prove the guards are effective: omitting the
native cached-format validation accepts an invalid retained texture and fails
the new regression; restoring the old whole-group private override omits the
companion and fails the dependency regression. Both sources were restored byte
for byte and SHA-256 checked before the broad targeted gate. The retained
compile failures involved a new fixture's crate imports/test-helper signature
and an attempted private Scene field access. Existing assertions, skips, pixel
bounds and product refusal policies were not changed to pass acceptance.

| Targeted gate | Actual result |
| --- | --- |
| `native-mask-contract` | 1 passed; zero failed/ignored |
| `frame-imports` | 3 passed; zero failed/ignored |
| `render-imports` | 3 passed; zero failed/ignored |
| `frame-pixels` | 2 passed; zero failed/ignored |
| `import-pixels` | 2 passed; zero failed/ignored |
| `curves-masks` | 14 passed; zero failed/ignored |
| `effects-library` | 32 passed; zero failed/ignored |
| `desktop-check` | Passed |
| `render-library` | 262 passed; zero failed/ignored |
| `render-native` | 99 passed; zero failed/ignored |
| `host-memory-policy` | 17 passed; zero failed/ignored |
| `ir-default-output` | 1 passed; zero failed/ignored |
| `private-numeric` | 1 passed; zero failed/ignored |
| `private-precision` | 1 passed; zero failed/ignored |

The original broad attempt terminated abnormally in the existing CPU-only
pointwise lifetime test (Windows STATUS_ACCESS_VIOLATION). The same unchanged
executable passed that test in isolation. The complete native scope was then
rerun, followed by the remaining host/worker/private-data gates. The receipt
combines only successful scopes on byte-identical source, with explicit original
receipt/log references; it is not represented as one uninterrupted green run.
The original crash and Windows fault context are retained. Its root cause is
unconfirmed and remains a final-acceptance follow-up, not a claimed product fix.

These scopes contain 430 distinct parent tests. The three
library filters and one repeated mask-error target are not counted twice. The
default worker gate still verifies six real typed failures, file/Error-stderr
output at the default level, stdout isolation, prior pixels and recovery. The
existing private-instance companion retains 16 updates and eight reopen,
isolation and undo cases; those assertions were unchanged.

A final native companion compiles and runs the cached-mask contract test after
rustfmt corrected only the leading indentation of its closure arguments. The
receipt verifies that exact named-file/line change, identical remaining source
hashes and unchanged assertions; the earlier broad source is retained explicitly.

The source-bound acceptance froze 140 Rust/Cargo hashes and checked
them after every step. No panels, styles, UI interactions, dependencies,
third-party assets or Android source were changed. The
[verified frame-import checkpoint](data/typed-render-dag-frame-imports-cbe265a2.zip)
retains successful/failed attempts, source snapshots, actual logs and native
receipts, red controls, exact restoration proofs and controllers. Size:
8,452,791 bytes; 1663 files plus manifest. SHA-256:
`269671eb6d37d7073f2308c8a635c83c748bac08b500fcd921591b54055cd29a`. Every archive entry was read back and hash-verified.

This is targeted checkpoint acceptance, not G or issue closure. Full producer/
value/ROI ownership before placement, IR-level affected-group scheduling,
end-to-end shader/compile/pass/frame/physical-peak measurements and final
main-aligned integrated regression/cross-platform desktop CI remain open.
Android implementation and acceptance remain excluded. #78 stays open,
followed by #74 and #87.


## Pre-raster semantic ownership checkpoint (2026-10-26)

The frame-local semantic graph now exists before mask rasterization, vector
tessellation and generator preparation. It owns raw layer views, sampled image
selectors, ordered authored effect producers, initial mask materialization and
explicit terminal materialization. Composition/layer/effect origins, working
color/alpha/precision contracts and logical region recipes are retained there.
The native builder consumes its operation order and source/region rules rather
than independently rediscovering them from the scene or completed pass clocks.

Physical proposal admission records real native ranges and texel grids. An
ordinary preview refusal or zero-opacity identity aliases its semantic output
to the actual accepted upstream value; subsequent padding/bounds recipes use
that value. A generator retains its composition-space output recipe. Continuous
vector terminal materialization refines the source grid at the actual raster
density. No fabricated 1x1 extents or all-effects-succeed rectangles are used.

Concrete execution lowering compacts those admitted producers and aliases.
Pass clocks and native uniforms verify ownership, canonical boundaries, region,
extent and sampled selector encoding; they cannot invent producers or switch
raw/effected dependency identity. Real shader identities, costs, package/frame
resource imports, checked topology, lifetimes and the existing physical allocator
remain authoritative for their respective lowering stages. Logical canonical
source views describe SDK sampling/conversion results, not the stored byte
encoding of external assets. Existing raw-asset allocation/binding ownership is
retained; no new exclusive GPU resource or device is assigned per node.

Three new regressions verify pre-raster ownership/frozen recipes and alias
regions, rejection/recovery of forged clocks or native output regions, and
rejection/recovery of altered backend resource selectors. Existing ordinary
preview bypass, early cycle, diamond/snapshot, mask/generator, numeric pipeline
reuse and four-precision pixel assertions remain unchanged. Two unchanged-test
negative controls remove the clock/selector guards and fail at unwrap_err;
exact accepted source bytes are restored afterward.

| Receipt under artifacts/pr-queue/ | Actual result |
| --- | --- |
| issue78-numeric-semantic-frame-v1 | Failed compile in the new test fixture (field and enum names); retained, no pass claim |
| issue78-numeric-semantic-frame-v2 | Early preparation/lowering filters: 23 passed before the additional frozen-origin guard; historical scope |
| issue78-numeric-semantic-frame-v3 | Exact final-source targeted checkpoint: 265 render library, 99 native across 21 targets, 32 effects library, 17 host memory-policy, 14 curve/mask, two frame-import GPU, two package-import GPU, and three desktop worker/private-data tests; desktop FFmpeg check passed |
| issue78-numeric-semantic-frame-clock-control | Removed clock ownership guard: unchanged new regression fails as expected |
| issue78-numeric-semantic-frame-selector-control | Removed typed selector guard: unchanged new regression fails as expected |

The final checkpoint contains **433 distinct parent test passes**, zero failures
or ignored tests. Focused filters are library subsets; mask_error_boundaries is
deduplicated across the two native groups. Child-process cases are not counted
again. Actual artifacts retain 16 private/frame numeric pixel rows, 12 curve/mask
rows, 16 desktop numeric updates, eight reopen/isolation/undo cases, and six
default-level IR fault/recovery cases with Error file/stderr and stdout isolation.
The earlier unexplained CPU-planning access violation remains recorded in the
preceding checkpoint; it did not recur in this source-bound native range.

This is targeted acceptance, not G or cross-platform completion. No panel,
UI/style, Android, dependency, compatibility or third-party asset change is
included. Final cost evidence, main alignment, full integration and maintained
desktop CI remain pending.

Evidence: [verified semantic-owner archive](data/typed-render-dag-semantic-owner-97936c6b.zip), SHA-256 `9324789ab25942c168b3bb73933d6d245ad14e0c5d6b8be0fdbd093302c42727`; 4,152,536 bytes, 799 payload files plus manifest, all entries read back and hashed.


## Independent-group compilation and measured costs (2026-10-26)

After the semantic-owner checkpoint, the branch was fetched/rebased without
conflicts onto main `880739de`; its semantic-owner head became `74940bff`.
The earlier source-bound counts remain historical acceptance, not acceptance
of all newly integrated main changes. A checkout-only CRLF byte change in
effect_plan.rs is distinguished from 21 main source-content changes in the
archived rebase receipt. G must cover this integrated baseline.

The new native multi-group regression uses two independently addressed layers,
each with a proved Invert -> Tint group. In every working precision, two numeric
edits (one per group) add zero portable compiles and zero native pipeline builds.
Reversing only the first group's structure preserves the second group's native
program and pipeline. UNORM8/Float16/Float32 add one portable compile and one
native fused pipeline; UNORM16 correctly retains its quantized transfer boundary,
adds no portable group compile, and first builds the two original effect
pipelines plus their working conversion. Restoring the original structure
reuses both portable and native code. Full-frame linear-premultiplied pixels
are compared with actual ungrouped rendering in every phase; no existing
assertion, tolerance or return behavior was changed.

This closes the literal issue requirement for affected-group **compilation**.
It does not introduce an extra requirement to skip unaffected GPU work in a new
frame: graph operations conservatively carry CurrentSample time dependence and
still execute. Likewise, CPU preparation may parallelize; the pure vector
preparer does so. Mutable mask/generator caches and ordered budget admission
remain serial; they are retained behavior, not an unimplemented mandatory new
parallel executor.

`cargo run -p motion-render --example render_dag_costs -- <output.json>` runs
the same independently checked fixture at 65x33 and 513x257, all four working
precisions. The optional example is outside the default test matrix. The source-
bound run contains eight cases and 40 paired grouped/ungrouped phases; a separate
one-parent-test regression runs all four precisions at the smaller extent.
Observed values below come from the dev profile on this Windows adapter, with
the exact sources, commands, environment overrides and raw JSON archived. A renderer-cold
compile does not imply cold driver caches. Two warm numeric samples per case
are observations, not a statistically established speedup or FPS baseline.

| 513x257 working mode | One reusable group WGSL bytes | Portable compile ms | Renderer-cold native compile ms | Warm draw/readback wall ms (two samples) | Warm admitted peak bytes |
| --- | ---: | ---: | ---: | --- | ---: |
| unorm8 | 5851 | 9.994 | 14.989 | 1.897–2.135 | 4,049,920 |
| unorm16 | 5733 | 10.389 | 15.038 | 2.372–2.605 | 6,158,348 |
| float16 | 6539 | 11.738 | 16.748 | 2.418–2.434 | 6,158,348 |
| float32 | 5661 | 11.166 | 15.688 | 2.953–2.958 | 10,375,204 |

Each initial pair measures 14 -> 6 native effect passes. Shader byte lengths
and portable compile time describe one actual generated program reused by two
groups; repeated group metadata must not be summed as distinct compiles.
Native timers are cumulative CPU pipeline creation/validation time per renderer,
with phase deltas available in JSON. Draw/readback wall intervals include CPU
preparation, submissions and synchronization; they are not isolated GPU time.

The capacity measure is the actual central budget's completed per-frame
`session_enrolled_native_capacity` window, including retained resources and
command reservations. Its API explicitly reports complete_coverage=false and
is_physical_vram=false. It excludes un-enrolled/driver allocations and process
RSS; independent category maxima must not be summed. All recorded windows have
zero pending or unconfirmed submissions and zero omitted tracking windows.

Attempt costs-v1 failed because the new fixture incorrectly expected complete
physical coverage from that deliberately partial gauge. The fixture now asserts
the reported scope/false flags and preserves the completion/fault/omission
guards, recording the limitation rather than changing production accounting.
costs-v2 passed the initial multi-group regression; costs-v3 passed the final
stronger native invalidation assertions and the full optional cost example.
All attempts and exact source snapshots are retained. This is not G: full final-
source regression, maintained-desktop CI and issue closure remain pending.
No UI, panel, runtime feature, dependency or Android change is included.

Evidence: [verified cost archive](data/typed-render-dag-costs-74940bff.zip), SHA-256 `7c6923f629a79e08ce582192ccd11fd9202f3effc559b460a458ab82a1173ad5`; 2,662,213 bytes, 552 payload files plus manifest, every entry read back and hashed.


### C1: owned logical lowering, native drafts confined to admission

The runtime effect lowerer consumes the pre-raster semantic graph plus owned
native admission annotations. Its signature has no EffectFramePlan, scene,
pass/uniform table, clock table or compiler. Native annotations bind actual
code identities, resource imports, costs and admitted pass ranges to producers
already owned by the semantic graph. Primary and sampled-layer dependencies
come from that graph; matte ownership is captured before rasterization and
native matte drift is rejected before code hashing. Task mapping uses owned
draw ranges. The public snapshot diagnostic adapter still accepts a native
draft to diagnose public-draft mutations; its lowerer consumes an owned graph
and is not the authoritative runtime dependency source.

The new regression releases the native drafts, scene and compiler tables after
admission, then actually lowers the retained semantic graph and compares the
complete serialized plan and bindings against the admitted original. A second
regression proves native matte drift cannot redefine semantic dependencies and
verifies recovery after restoring it. Existing assertions and tolerances are
unchanged. Source-frozen Windows runs passed 267 render library tests and 16
native integration tests (snapshot execution, runtime templates, pointwise
fusion, independent group invalidation and extreme alpha), zero failures or
ignores. Two existing non-test-library dead-code warnings are retained.

This evidence closes C1's lowerer dependency, not C3, the five independent
fusion proof obligations, the Linux Float16 frame-import failure, or G. The
244a7b39 baseline full-workspace run cannot accept these changed sources.
No panel, UI, dependency or Android changes are included.

Evidence: [C1 source and raw acceptance receipts](data/typed-render-dag-c1-244a7b39.zip), SHA-256 `5ad3f71be97b1b25f1795a97519d46ac51e6c8bc9fcb43014233fab4b5812870`; 278,989 bytes, 34 payload files, all archive entries read back and hashed.


### C3a: enforce finite measured code and bounded static-work admission

Compiled shaders now carry a conservative fragment texture-read bound derived
from validated Naga IR. It includes ImageSample/ImageLoad expressions, SDK
helpers, repeated function call occurrences and both branch arms. Naga's
callee-before-caller invariant permits iterative expansion without recursion.
The existing portable-source bounded-loop product is applied once to the
expanded call graph; current SDK helpers have no loops. This overestimates
branches, zero-iteration loops and sibling loops. It counts shader read
operations, not filtering taps, covered pixels, GPU instructions or duration.
Overflow reachable from the fragment refuses compilation; unreachable helper
overflow is not charged to an unrelated entry point. Unknown implementations
retain None and cannot pass runtime sample-bound admission.

Native effect/snapshot and reusable-template checks now enforce 64 MiB aggregate
compiled text, 1,048,576 aggregate static-loop work and 16,777,216 conservative
fragment reads per checked graph. These are finite application safety policies,
not adapter measurements. Actual per-pass code identities/text sizes, loop work
and read bounds flow into the graph before checking. Source/import nodes cost
zero known reads. Structure-only package validation has no compiled costs and
is explicitly separate from runtime admission. Fused-program read metrics also
come from the compiled IR instead of a literal two-sample claim.

New independent tests check loads, SDK curves, repeated calls, branches, nested
loops, reachable overflow, unreachable work, all three individual budget
refusals, unknown-cost refusal and recovery. Two previous assertions expecting
unknown sampling have been strengthened to require actual known bounds; no
numeric tolerance, refusal guard or GPU output assertion was relaxed. The first
new overflow fixture accidentally named a helper f32; that source-name collision
was corrected without changing its assertions. Its failed receipt is retained.
The frozen corrected run passed 39 effects tests and 273 render tests, zero
failures or ignores, including the actual native diamond and runtime template.

C3 remains open for the runtime three-slot ceiling. Independent fusion proofs,
the Linux Float16 frame-import failure and final-source G also remain open.
This is a logical admission change, with no UI, panel, dependency or Android work.

Evidence: [cost-bound source and raw receipts](data/typed-render-dag-cost-bounds-78f6205c.zip), SHA-256 `0fcdff7a4a6ac8b1a992f79001e96fb000b3298bf5cdddd0e70ebb089591d313`; 508,002 bytes, 83 payload files, all read back and hashed.


### C3b: runtime branches use real physical byte admission

Runtime templates no longer allocate from three fixed working slots. Every
checked package pass has a bounded virtual image name in its existing storage
bank. The def/use graph determines immutable image lifetimes; the physical
placer reuses compatible storage only after last use. Canonical and utility
names keep their prior meanings. The 8-entry semantic-maxima diagnostic is
not an allocator: native allocation, resource reservations and packed-plan
pass bindings still use the actual physical descriptor/binding table. Virtual
names are bounded by the existing eight-pass package limit and the 32-bit
diagnostic mask, not a three-texture policy.

A functional change required by this budget inversion is explicit: valid
four-live-image branches now execute when their real byte capacity fits.
The old fixed-slot refusal test has been strengthened into an actual byte-
budget refusal: four 1536x1536 Float16 images require exactly 72 MiB against
64 MiB, while the healthy three-image diamond fits. Original pipeline, old-
pixel preservation and recovery assertions remain. Its obsolete Storage
{required:4, maximum:3} assertion is replaced by the typed EffectScratch
Budget cause with exact byte values; the new small-image control actually
executes four simultaneous images in all four precisions, independently
checks their pixels and verifies packed passes reference four real physical
allocations rather than virtual names.

The first refusal run exposed a real functional defect: a template's physical
Resource refusal entered ordinary best-effort preview bypass and returned Ok
after removing the effect. Template resource refusals now propagate their
original structured cause and layer/effect context before native compilation
or target writes. Ordinary non-template preview fallback is unchanged. The
next attempt caught a misplaced local binding at compile time; both failed
attempts are retained, with no weakened output assertions.

The final frozen run passed 286 tests: three runtime-template regressions,
two actual Vulkan frame-import tests, 267 render library tests and 14 native
snapshot/group/fusion/alpha regressions, zero failures or ignores. The Linux
CI Float16 import failure remains unresolved: this Windows Vulkan run is not
a substitute for its Mesa result. Its existing 0.001 tolerance is unchanged;
the assertion now reports the exact worst coordinate/channel and pixel pair.

C1 and C3 implementation/targeted acceptance are now recorded. The five
independent fusion obligations, Linux pixel failure and final-source G remain
open. No panel, UI, dependency or Android changes are included.

Evidence: [runtime storage source and raw receipts](data/typed-render-dag-storage-acc16781.zip), SHA-256 `1dee20491445ac173943729f7a02bf49ba4c6e5c542793e61d79b7024e238e93`; 591,861 bytes, 59 payload files, every archive entry read back and hashed.

### Completed baseline whole-workspace gate at 244a7b39

The clean frozen source at 244a7b39, based on main 880739de, passed the
maintained workspace with all GPU feature gates, diagnostics and desktop FFmpeg:
3,011 passed, zero failures, ten pre-existing ignored optional studies, 298 test
targets. Panel style checks, the desktop check, a real desktop binary build and
an isolated actual command-list invocation also passed. The catalogue parsed as
protocol JSON and the default file log was created. No new test was ignored.

This is baseline evidence only. It predates C1, C3, the new fusion obligations
and the Float16 store fix, so it cannot close G for those later changes.

Evidence: [baseline source and raw receipts](data/typed-render-dag-baseline-g-244a7b39.zip), SHA-256 `03df72fe629d3ae1c5baf91982544df98c476615bf5ccc9e8d8f62a9ba925686`; 2,872,256 bytes, 187 payload files, all archive entries read back and hashed.


### Independent fusion obligations and explicit Float16 effect stores

The [five independent arguments](pointwise-fusion-obligations.md) separate
scalar shader identity/evaluation, ordered quantization, alpha, external image
and matte readers, and compositing/backdrop consumers. Each has its own frozen
native gate and raw output. Boundary retention is distinguished from supported
fusion; bounded finite pixel probes are not a universal floating-point theorem.

The scalar GPU gate compares actual pinned original bodies with generated
functions bit-for-bit on 1,728 inputs in 25 parameter/channel cases, and rejects
mutated compiled implementations. The independent binary16 oracle covers all
63,488 signed finite half cells and 253,952 scalar center/quarter/tie probes;
190,465 differing results in the omitted-quantization control prevent a copied
input from passing. The exact half helper is shared by native and fused stores.

A newly added procedural RGB-only/alpha-preserving HDR probe initially failed:
Float16 worst error was 0.005859375 against the unchanged 0.005 bound. Neither
an identity-conversion shortcut nor exact pixel-center sampling corrected it;
both unsuccessful production experiments were removed. A later first integration
attempt caught a duplicated helper definition and is also retained. No output
assertion, tolerance, or existing test was relaxed.

The functional numeric fix explicitly rounds Float16 SDK effect outputs to
representable nearest-even values before attachment conversion. Native format
conversion alone cannot establish this policy. Admitted output format selects
a bounded native pipeline specialization; full helper source participates in
code identity/cost accounting, specialization cannot exceed admitted text size,
unknown/multiple policy markers refuse, and source ownership is not mutated.
This policy covers effects, including utility conversions, not hardware blend
rounding. There are no new uniforms, dependencies, UI or Android changes.

The final source-frozen nine-step gate passed 44 tests with zero failures or
ignores. The native alpha gate now independently requires exact alpha in all
16 RGB-only cases, plus the existing 32 exact retained alpha-lifting cases.
UNORM16 transfer-order, effected-resource/matte and three compositing families
retain their original stores with exact pixels. The ordinary four-mode chain
remains a separate positive fusion control. The SDK specialization also has
valid WGSL/GLSL and refused-marker/recovery coverage. This targeted evidence
does not accept subsequent main changes or replace final G. Linux CI's existing
Float16 import bound of 0.001 still requires a new remote run of the fix.

Evidence: [five obligation source and raw receipts](data/typed-render-dag-five-obligations-41d517c9.zip), SHA-256 `7aa975965a948ebc88d905e6a0ea1625d4ae0f4cff96c45b7f0029f19879ead9`; 1,057,709 bytes, 101 payload files, every archive entry read back and hashed.

### Software Vulkan identity sampling and pixel coverage

The next actual [Linux Rust/GPU run](https://github.com/MAD-Producer/Kilonova/actions/runs/40286617249/job/121451675784)
passed the previously failing Float16 mode but exposed a Float32 import error:
2.9206276e-6 exceeded the unchanged 2e-6 limit. The exact same pixel, channels
and magnitude were reproduced locally with a software Vulkan ICD. Native
filtering of rounded normalized coordinates mixed neighbouring texels in an
authored identity draw; normalized edge subtraction also lost full coverage
at the final row/column. Merely evaluating all bilinear taps in f32 did not
correct it. Reconstructing the logical 2D projection alone corrected interior
pixels but left an alpha error of 2.7418137e-6 at the bottom-right corner.
Both failed experiments and the original CI log remain in the evidence.

The functional correction reconstructs the known logical composition projection
in f64 for canonical UVs; supplied camera/custom projections retain their
original matrices. The CPU separately proves positive, axis-aligned rectangles
with exact integer physical edges, using equality rather than an epsilon snap.
Their box coverage is binary. Only when the physical extent also equals the
used source-texel extent can sampling use the exact texel at that pixel centre.
Scaled, fractional, rotated and projective mappings keep linear filtering and
the existing general edge coverage. Proof metadata uses two previously unused
uniform lanes; there is no new allocation, binding or uniform byte. Portable
programs conservatively retain their ordinary sampling path.

The frozen software-Vulkan gate passed 52 tests, with no failures or ignores:
the five obligations were re-executed on this adapter class, including the
original four-precision import test and its original bounds. A new independent
four-colour GPU fixture verifies exact identity pixels, fractional coverage and
bilinear mixing under both quarter-pixel translation and integer-edge scaling.
A nearest-filter negative control cannot satisfy the scaled result. CPU checks
exclude fractional edges, fractional extents, rotation, perspective and flips
from the fast path; four existing projected-plane regressions also pass.

This scoped gate does not replace final whole-workspace or remote acceptance.
The earlier final-G v2 run was explicitly superseded once the CI failure was
reproduced; its partial output is not counted as successful final acceptance.
Final G must be repeated after this correction on the rebased branch.

Evidence: [software Vulkan source and raw receipts](data/typed-render-dag-software-vulkan-1ddb33af.zip),
SHA-256 `c8f486700bfa092782624462914eae82607f41535465ff3ddd2a668686be8d59`;
343,400 bytes, 53 payload files, every entry read back and hashed.
