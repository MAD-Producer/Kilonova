# Frame memory admission: issue #61

## Scope and baseline

The starting source is main `26b834f7a73918744884708851b6c6dc8ad567f0`,
including the merged current project-format contract. Review the current issue
requirements before treating older partial-budget work as completion. #60,
#61 and #67 remain open. The separate unpublished precision branch is preserved;
no project precision setting or 3D integration belongs to this increment.

## Requirement audit

| Requirement | Existing evidence | Remaining work / proof required |
| --- | --- | --- |
| Configurable category allowances | `resource_policy.rs`: source, effect scratch and preview front/candidate allowances | Introduce combined admission; independent limits do not bound their sum |
| Resource identity and alias accounting | `resource_ledger.rs`: process-local texture identities; merged references count once | Account for every admitted allocation and retained lifetime, including caller-owned targets and buffers |
| Current plus pending ownership | `frame_memory.rs`: explicit identity union; `submission_ledger.rs`: opt-in bounded metadata snapshots | Observation is not reservation or frame peak; cover untracked/pending-overflow paths before claiming a total bound |
| Typed refusal | `resource_error.rs`: budget scope and required/budget bytes distinct from other errors | Preserve typed causes at combined-admission boundaries; do not downgrade shader, codec or device faults |
| Bounded preview degradation | `preview_memory.rs` and host `Session::render_with_view` | Exercise combined pressure through eviction, proxy/target reduction, finite retries, hysteresis and cooldown |
| Last valid image | `preview_targets.rs` and host retained presentation | Refusal/rollback must preserve the submitted front, frame identity and project state |
| Full-quality export | Frozen session policy and separate PNG/movie renderers | Admission must reject explicitly instead of inheriting preview resolution/effect changes |
| Platform budget/pressure | `scratch_budget` uses system-memory tiers and guarded/unknown defaults | Query supported platform pressure with explicit provenance; unknown remains conservative, not fabricated GPU capacity |
| Repeated lifecycle changes | Existing ledger/policy tests cover selected replacement and reset paths | Exercise resize, seek and project replacement with actual combined reservations and retirement |

The current ledger documentation explicitly excludes raw submissions, detached
renderers, standalone conversions, GPU buffers, upload staging, swapchains and
external decoder/driver allocations. A zero known subtotal, dropped application
handle or completed metadata snapshot must not become evidence that those
allocations have physically retired. CPU caches and driver/decoder allocations
need separate scopes; application logical bytes are not measured device VRAM.

## Implementation and acceptance order

1. Audit #60 ownership coverage at actual allocation and submission boundaries;
   distinguish logical capacity, CPU cache and external/unknown pressure
2. Add combined reservations with checked capacity, allocation identity,
   atomic refusal and rollback; keep existing category limits independently
3. Preserve admission across pending GPU use and retained views/bindings;
   release only the lifetime actually covered, without claiming driver residency
4. Connect preview/refusal recovery and full-quality export before treating the
   reservation primitive as delivered product behavior
5. Validate actual low-budget failures, original fault propagation, bounded
   recovery, last-frame retention and repeated lifecycle changes
6. Complete #67 lifetime/size/format/usage scratch reuse after this foundation;
   resume #72 precision selection and #87 scene integration afterward

This audit is a work plan and source finding, not new allocation enforcement,
passing acceptance or closure of an issue. Record implemented paths, source-bound
tests, output artifacts and remaining limits here as the work proceeds.

## First implementation increment

`AllocationBudget` provides atomic, shared logical-capacity admission with typed
refusals, checked arithmetic, bounded metadata and alias-safe leases. Offscreen
targets use their actual texture format; capture targets also reserve aligned
readback capacity. Views and the admitted Presenter source retain their lease.
Admitted command encoders conservatively retain the budget's enrolled allocations,
including new allocations during encoding, through command-buffer abandonment or
queue completion. Submission rejects a different budget or rendering queue.
This lifetime protection is independent of optional diagnostic snapshots.

Renderer-owned draw, exposure, frame-blend, layer-raster and adjustment submissions,
host normal/temporal/retained presentation and desktop UI capture use these scopes.
Lowering a limit preserves existing reservations and permits presentation of an
already admitted front; content work and new allocations can refuse. No panel,
project-format, precision-setting or 3D behavior is changed in this increment.

The peak is conservative admitted logical capacity, not exact frame accesses,
physical device residency or a complete application GPU-memory bound. Source/video
uploads, scratch textures, other buffers, external/raw submissions and driver or
decoder allocations are not yet enrolled. Upload enrollment must preserve pending
queue-write lifetimes as well as command-buffer lifetimes. Session-wide budget
configuration, platform pressure, preview recovery and export policy integration
remain pending. #61, #60 and #67 must remain open.

## Accepted local evidence for this increment

Production implementation: `66d8b8f8b986c6dea2613b4b25bae0dd0844cc62`.
Additional fault-injection tests: `c511bb51e69873a472a3e1ac3d552c8807e33188`;
the only intervening source change is `allocation_budget/tests.rs`. This is
Windows local acceptance, not cross-platform CI or complete #61 acceptance.

| Check | Result | Source |
| --- | --- | --- |
| Workspace/all-targets FFmpeg check, excluding Android | Pass, existing warnings retained | `66d8b8f8` |
| Entire render library | 57 pass | `c511bb51` |
| 19 selected render integration suites | 107 pass | `66d8b8f8` |
| Host failure boundaries, memory policy and preview memory | 17 pass | `66d8b8f8` |
| Actual FFmpeg movie/export round trips | 17 pass | `c511bb51` |
| Desktop node preview and process failure boundaries | 6 pass | `c511bb51` |
| Panel style consumer guard | Pass; no panel file changed | `c511bb51` |

The selection contains 204 distinct passing cases, zero failed or ignored. The
earlier 56-case render-library execution is superseded by the 57-case execution
and is not added again. The 12 new cases provide these specific proofs:

- #61-A01/A02/A03: alias identity, atomic refusal and recovery; lowered limits;
  eight real concurrent reservation contenders cannot spend the same allowance
- #61-A04: actual poisoned state, missing identity and underflow fail closed;
  release logs Error once per affected budget at Info level after unlocking
- #61-A05: checked descriptor capacity covers mipmaps, arrays, volume depth,
  compressed blocks, multisampling, unknown formats and overflow
- #61-A06/A07: two renderers share actual offscreen capacity; capture refusal
  rolls back; open, finished, submitted and separately abandoned encoders retain
  reservations without diagnostic snapshots
- #61-A08/A09: another budget or queue refuses before submission; the 64-scope
  metadata ceiling recovers after abandonment
- #61-A10: a Presenter retains its source after the target is dropped; actual
  GPU rendering and readback preserve green pixels after lowering the allowance
- #61-A11/A12: actual Float16 target capacity is eight bytes per pixel; aligned
  readback capacity is admitted and actual UNORM8 capture pixels remain correct

Existing desktop tests additionally exercise actual command failures through
the process logger at default Info: original causes reach replies, file logs
and Error stderr without leaking log text into stdout. Existing render, temporal,
nested, preview and video assertions were retained; four Presenter call sites
only unwrap the newly fallible draw result. No full GUI launch or new UI change
is claimed. Movie tests use the existing local LGPL runtime
`n8.1.3-16-ge0a878dd70-20261009`, not pinned release qualification.

Logs: `E:/Dev/aem/artifacts/pr-queue/frame-admission-final-*.log`.
Hash receipt: `E:/Dev/aem/.tools/issue-closure-review/frame-admission-acceptance.json`.
Retain intermediate failures separately: initial raw-view integration errors,
the frame-blend Rust borrow error, and a movie command selecting the wrong
package. The corrected movie command executed all 17 selected tests; the command
failure is not a product failure or a passing test. Earlier three-unit/six-GPU
passes are intermediate evidence only. Build and temporary directories stayed
on E, with one own Cargo build at a time.

This accepted increment does not close #61, #60 or #67. Next integrate the latest
main, enroll the remaining resource and queued-write lifetimes, and connect the
session-wide policy and pressure/recovery paths before claiming unified coverage.

## Second implementation increment: source destinations and cached bindings

This increment enrolls the startup white image (four bytes), static/RGBA uploads,
converted video outputs, frame-blend cached outputs and same-frame effect-input
snapshots in the same admission budget as render/capture targets. Existing source
category limits remain separate. A replacement reserves its full destination
before discarding the old image. Same-sized uploads reuse the existing destination.
Allocation failures retain typed budget causes, required/allowed bytes and source
or video object/source/timestamp/phase context.

Queue writes require a separate lifetime boundary: `write_texture` stages work
but does not submit it. A renderer journal deduplicates allocation identities,
records the destination before the write and retains reservations across cache
eviction. A subsequent renderer submission transfers them to an actual queue
completion callback, independently of diagnostic snapshots. Device polling alone
does not prove that unsubmitted writes have completed. Teardown submits remaining
writes once without blocking and hands leases to completion; this is a teardown
flush, not a retry, timeout or worker restart. It also covers callers retaining
raw device/queue clones after dropping the renderer.

Cached effect input/source and image-resource bind groups retain the matching
source reservation until their actual binding is released. Renderer-owned
standalone YUV conversion uses an admitted encoder; reserve its scope before
replacing the previous RGBA destination so a scope-cap refusal preserves the
old image and completed stamp. Upload-journal poison refuses new writes, preserves
known reservations for cleanup and reports Error once per journal after unlocking.

The seven earlier allocation-budget integration cases now explicitly include the
four-byte startup image for each renderer. Their expected destination/readback
capacity, shared-renderer live counts and post-retirement baseline change by that
actual enrolled resource; pixel, scope-cap, lifetime and refusal assertions remain.
This is accounting for a newly enforced resource, not a relaxed acceptance limit.

New cases cover six actual GPU source/upload lifetimes, actual retained effect
bindings, poisoned cleanup with suppression and a real combined-budget failure
through the desktop reply/file/Error-stderr boundary at default Info. The latter
also checks recovery and stdout isolation. Focused intermediate passes are retained
separately; final source-bound acceptance will be appended after the committed
source has completed the selected regressions.

Remaining limits are explicit: YUV planes and uniforms, effect/mask/vector/plugin
scratch and other buffers are not all enrolled. The queue journal covers admitted
destinations, not backend upload staging. Detached `PreviewInputTexture` conversions
and caller-owned/raw GPU entry points remain outside this guarantee. A raw queue
submission does not retire journal metadata early; the journal conservatively
retains it until a renderer submission or teardown. Shared session configuration,
platform-pressure provenance, combined preview recovery, full-quality export policy
and repeated project/seek/resize acceptance remain pending. #60, #61 and #67 remain
open; #72 project settings and #87 integration remain parked behind them.

## Accepted local evidence for the second increment

Production and nine new cases: `306e4f7a`. Final checked source:
`4b5cc19870e586ccbbc19c3813ea2e0a1e86a3fd`, including main
`4f80293bc480bfd37057bfe5bb3366567b835f89` and its export-freeze lifecycle
fix. No source changed during the sequential acceptance run.

| Check | Result |
| --- | --- |
| Workspace/all-targets FFmpeg check, excluding Android | Pass; existing warnings retained |
| Entire render library | 59 pass |
| 29 selected render integration suites | 150 pass; one existing optional probe initially ignored |
| That optional image probe, explicitly selected with `--ignored --exact` | 1 pass; generated 4096x64 PNG, actual 2048x32 cold/warm proxy bytes agree |
| Host failure boundaries, memory policy and preview memory | 17 pass |
| Actual FFmpeg PNG/movie/export round trips | 17 pass |
| Desktop node preview | 3 pass |
| Desktop process failure boundaries at default Info | 4 pass |
| Panel style consumer guard | Pass; no panel file changed |

There are 251 distinct selected passing cases, zero failures and no unresolved
ignored case in this selection. The ordinary image-resource invocation's existing
optional probe is recorded as ignored in that invocation, then as executed and
passed separately; an ignored result is not counted as success. These are Windows
local checks, not the entire desktop test suite, a full GUI acceptance or
cross-platform CI. Existing native codec/compiler warnings remain in the logs.

| New case ID | Evidence and boundary |
| --- | --- |
| #61-A13 | Evicted, unsubmitted upload destinations retain capacity; same-destination writes deduplicate; polling alone does not retire them; actual renderer submission does |
| #61-A14 | Real green pixels survive refused source growth; same-sized blue updates reuse capacity at the exact limit; increased capacity permits recovery |
| #61-A15 | RGBA and YUV-output admission preserve prior completed stamps on refusal; real conversion retires through the submitted encoder lifetime; YUV planes remain outside admission |
| #61-A16 | Sixty-four actual open encoder scopes refuse standalone conversion before replacing the prior RGBA image; dropping scopes permits the same conversion |
| #61-A17 | Three renderer teardown cycles with external queue/device handles retire pending destinations after actual GPU completion without capacity drift |
| #61-A18 | Actual startup white-image admission refuses a three-byte allowance with source/phase context and leaves no reservation |
| #61-A19 | Actual tint output and cached effect bindings retain the removed source's reservation until invalidation, then release exactly its 64 bytes |
| #61-A20 | Controlled upload-journal poison retains known destinations, refuses new tracking, logs Error once at Info outside its mutex and retires through the actual queue |
| #61-A21 | An actual combined-budget refusal preserves its typed scope and exact primary reply; request/operation/source/phase/bytes/time reach file and Error stderr at default Info, without stdout leakage; raising the limit recovers |

The additional regressions preserve existing assertions for real device loss,
validation faults, effect order/alpha, image working sets, masks/nested/exposure
pixels, preview front retention/recovery, video seek/cancellation/handoffs and
actual encoded output. The only earlier assertion updates account explicitly for
the newly admitted startup image, as described above.

Logs: `E:/Dev/aem/artifacts/pr-queue/source-admission-final-*.log`.
Source/log/fixture hash receipt:
`E:/Dev/aem/.tools/issue-closure-review/source-admission-acceptance.json`.
Focused pre-commit passes are separate intermediate evidence, not additional
distinct passes. Build/temp/artifact writes stayed on E and own Cargo/GPU commands
ran sequentially. No dependency or product asset was added. No PR or issue closure
is claimed for this partial increment. Continue resource enrollment and shared
session policy/pressure/recovery acceptance before #67 reuse work.

## Third implementation increment: YUV resources and authoring inputs

YUV conversion now reserves its real Y/UV texture descriptors and 48-byte
parameter buffer in the same combined budget as its output. Prepare resources
and track all queued-write destinations before replacing a cached input or
writing pixels. A late refusal must preserve the old RGBA image, completed stamp,
plane cache and original typed failure. Descriptor buffer capacity is logical
application capacity, not the backend's native heap size or residency.

An encoded upload token retains all four conversion reservations, including the
output, until abandonment or completion of the queue submission explicitly marked
by its caller. This also covers the existing raw-encoder API after cache eviction;
it does not rely on the caller using an admitted encoder. Callers must still
submit that encoder on the owning queue before marking the token, or discard both
on cancellation. Queued plane/uniform writes independently remain in the upload
journal until a renderer submission or teardown. Batch journal tracking acquires
its mutex once before any of those writes.

`PreviewInputTexture::new` now requires an explicit shared `AllocationBudget` and
returns a construction error instead of bypassing admission. Its copy uniform,
output, RGBA source, YUV planes and conversion uniform are enrolled. Its public
view is an `AdmittedTextureView`; cloning it retains the same allocation identity.
RGBA/resize/YUV preparation preserves the prior input until all capacity/scope/
journal checks succeed. Resizes and conversions submit with admitted lifetimes;
direct writes use the journal and its nonblocking teardown flush. Budget failures
are typed separately from invalid pixels and retain authoring phase/dimensions.
This is an intentional current Rust API change, with all repository callers
updated; no legacy constructor or implicit independent budget is retained.

The existing source-budget assertions include the newly enrolled Y4/UV2/uniform48
capacity for their actual 2x2 conversion. Pixel tolerances, refusal conditions,
scope limits and recovery assertions are unchanged. Existing standalone-input
tests only pass the explicit shared budget and unwrap construction; their
original assertions remain intact. Texture-only diagnostic subtotals still
exclude buffers; the combined admission counter includes them.

Six new cases cover actual late video-uniform refusal with pixel/stamp rollback,
raw-encoder token submission/cancellation, authoring constructor/shared-target
refusal, cloned-view lifetime across three teardown cycles, authoring source/YUV
refusal with pixel recovery and malformed-input classification, and the real
uniform refusal through the default file/Error-stderr/reply boundary. Record final
committed-source results separately after the full selected acceptance run.

The prior increments' unenrolled-YUV/authoring statements are historical scope
records, superseded by this increment only for these named paths. Other
renderer/effect/mask/vector/plugin buffers and scratch, queue-owned upload staging,
raw/external GPU scope, session policy sharing, platform pressure and complete
preview/export/lifecycle acceptance remain pending. Neither #60/#61 nor #67 is
closed by this resource enrollment; continue them before precision/3D integration.

## Accepted local evidence for the third increment

Checked production and test source: `7f6c81bdc9f2922dd1c7156482272d6e5079becf`,
including main `2803721b30ecbde1a6714ac74a6ca2267618f9b2`. The source remained
clean and unchanged throughout acceptance. An interrupted GPU invocation completed
22 suites; after confirming that its process no longer existed, the nine remaining
suites ran separately on the same source. The completed suites were not counted
twice, and an incomplete invocation was not reported as a successful exit.

| Check | Result |
| --- | --- |
| Workspace/all-targets FFmpeg check, excluding Android | Pass; existing warnings retained |
| Entire render library | 59 pass |
| 31 selected render integration suites | 158 pass; one existing optional probe initially ignored |
| Optional image probe explicitly executed with `--ignored --exact` | 1 pass; generated 4096x64 original and actual cold/warm 2048x32 proxy agree |
| Host failure boundaries, memory policy and preview memory | 17 pass |
| Actual FFmpeg PNG/movie/export round trips | 17 pass |
| Desktop node preview | 3 pass |
| Desktop failure boundaries at default Info | 5 pass |
| Panel style consumer guard | Pass; no panel file changed |

This selection has 260 distinct passing cases, zero failures and no unresolved
ignored case. The optional probe was executed separately, not counted as passed
because its ordinary invocation ignored it. This is Windows local evidence, not
the complete desktop suite, a new GUI launch or cross-platform CI.

| New case ID | Evidence and boundary |
| --- | --- |
| #61-A22 | Actual late YUV-uniform refusal preserves old pixels, stamps and capacity; an increased limit permits conversion matching CPU reference pixels |
| #61-A23 | Raw-encoder upload tokens retain the output, both planes and uniform after cache clearing, through abandonment or actual queue completion |
| #61-A24 | The authoring copy uniform and renderer targets share an explicitly supplied allowance; constructor and target refusal leave no leaked reservation |
| #61-A25 | Cloned authoring views retain the same allocation identity across teardown and completed pending writes, returning to zero through three real lifecycles |
| #61-A26 | Refused authoring source growth and late YUV-uniform allocation preserve actual pixels; recovery succeeds and malformed input remains a distinct fault |
| #61-A27 | Real YUV-uniform refusal preserves its original typed reply and reaches file and Error stderr once at default Info with request/operation/source/phase/bytes/time; stdout remains isolated |

Logs: `E:/Dev/aem/artifacts/pr-queue/video-plane-admission-final-*.log`.
Source, log and fixture hash receipt:
`E:/Dev/aem/.tools/issue-closure-review/video-plane-admission-acceptance.json`.
The earlier 204- and 251-case receipts remain separate historical evidence.
Own builds/GPU tests ran sequentially; build, temporary and artifact writes stayed
on E. No dependency, product asset, panel or existing pixel tolerance changed.
The three source-budget expectation changes account for the actual additional
Y4/UV2/uniform48 bytes. Remaining work listed above still prevents closure of
#60/#61/#67; neither project precision settings nor 3D integration resumed.

## Fourth increment: exposure accumulation admission

The exposure sample was already admitted. Its linear `Rgba16Unorm` mean texture
and 16-byte weight buffer now share the same combined allowance. Construction
stages all three resources before publishing the target; an early sample, middle
mean or late uniform refusal releases both the category reservation and every
temporary combined reservation. The typed cause includes extent, allocation
phase and the original buffer descriptor context where applicable.

Bindings and raw views drop before their owning resources and reservations.
Admitted encoder scopes retain all three allocations through abandonment or GPU
completion. The weight buffer is also tracked before each queued write, retaining
its destination independently of target ownership until submission or teardown.
Texture category subtotals remain twelve bytes per pixel and exclude the buffer;
the combined allowance includes the buffer once by identity. This is descriptor
capacity, not driver residency or upload-staging capacity.

Four new cases are prepared: staged refusal/rollback/recovery, abandoned and
finished encoder retention plus renderer teardown, refusal of an overlapping
target without corrupting completed phase pixels, and actual late uniform refusal
through the default file/Error-stderr/unchanged-reply boundary with controlled
request metadata. Existing assertions and pixel tolerances are unchanged. Record
committed-source acceptance separately; no passing result or issue closure is
claimed by this implementation description.

## Accepted local evidence for the fourth increment

Checked source: `1810fac1d0226ac78ac692096bdad3fb29d56dc3`, including main
`8385318aafe8bf9640d9e0db5e45665a6c18e227` and the accepted third increment.
The worktree remained clean and unchanged during sequential checks.

| Check | Result |
| --- | --- |
| Workspace/all-targets FFmpeg check, excluding Android | Pass; existing warnings retained |
| Eight affected render integration suites | 63 pass |
| Host failure boundaries, memory policy and preview memory | 17 pass |
| Desktop failure boundaries at default Info | 6 pass |
| Panel style consumer guard | Pass; no panel file changed |

This affected-path selection has 86 distinct passing cases, zero failures and no
ignored cases. It is Windows local evidence, not the complete desktop test suite
or cross-platform acceptance. The prior 260-case receipt remains historical
evidence for its exact source, not a claim that all those cases were rerun here.

| New case ID | Evidence and boundary |
| --- | --- |
| #61-A28 | Actual sample, mean and uniform refusals roll back category and combined reservations; exact capacity permits recovery without changing the texture-only subtotal |
| #61-A29 | Open and finished abandoned encoders retain all three exposure allocations; renderer teardown leaves only the externally owned target, then releases to zero |
| #61-A30 | An overlapping target's late refusal preserves the completed phase and capacity; subsequent red/blue accumulation matches real linear reference pixels and retires to the startup image |
| #61-A31 | Actual late exposure-uniform refusal preserves the typed cause and unchanged reply; request/operation/extent/phase/bytes/time reach file and Error stderr at default Info without stdout leakage |

Logs: `E:/Dev/aem/artifacts/pr-queue/exposure-admission-final-*.log`.
Each command has a source-bound exit receipt. Source and log hashes:
`E:/Dev/aem/.tools/issue-closure-review/exposure-admission-acceptance.json`.
Build, temporary and artifact writes stayed on E; no dependency, asset, panel,
existing assertion or pixel tolerance changed.

PR #395 publishes the first three increments. At this acceptance stage its Windows
CI had two native-presentation timeout failures; they are retained as failures, not reported
as green or dismissed as unrelated. The independent
`ci/desktop-failure-evidence` branch/PR #396 handles native failure evidence.
The exposure increment is not yet published. Other resource enrollment, shared
session policy, platform pressure, complete preview/export/lifecycle acceptance
and #67 reuse remain pending; #60/#61/#67 remain open.

## Fifth increment: dynamic effect resources

Effect scratch, dynamic curve LUTs, packaged PNG textures and per-pass parameter
buffers now use the renderer's combined admission budget. Partial scratch growth
stages replacements without moving existing slots or invalidating their bindings
until all new slots have been admitted. Texture format separation is unchanged.
The actual old/new overlap is charged, rather than subtracting a still-live old
pool from the peak. Refusals retain slot, extent, curve/pass index or bounded
package/path context and the original typed capacity rejection.

Queued LUT/package/uniform writes journal their destinations before writing.
The renderer and effect state share that journal, whose final owner flushes pending
writes at teardown. Cached input bindings now retain admitted scratch and LUT
identities alongside source images. Bindings and views drop before their owning
texture or buffer reservation. A LUT count change invalidates cached bindings,
including a shrink/grow sequence of preflights without an intervening draw; a
reused numeric index must not retain a replaced allocation.

Four new GPU cases cover later-slot refusal with unchanged old identities and
pixels, late parameter refusal with scratch reuse and grayscale recovery, LUT
refusal plus queued/abandoned destination lifetime, and real packaged PNG refusal
with recovery. Another process-isolated case exercises a real parameter refusal
through the unchanged reply and default file/Error-stderr boundary. Final
committed-source acceptance is recorded below. Existing assertions and pixel
tolerances remain unchanged.

Permanent effect fallback/identity textures and the sprite buffer, plus the
remaining layer/vector/mask/compositing/timing resources, are not enrolled by this
increment. Queue upload staging is still a separate pending scope. Shared session
policy, platform pressure, complete preview/export/lifecycle acceptance and #67
remain pending; this increment does not complete #61.

## Accepted local evidence for the fifth increment

Checked production source: `5c79786f99932531a3067d80e5a91c128b62c744`.
The source tree is identical to the original implementation commit `a318763b`;
the history was reconciled with the published PR #395 head `56e1dee9` and main
`374b7ea2`. The worktree remained clean and unchanged during sequential checks.

| Check | Result |
| --- | --- |
| Workspace/all-targets FFmpeg check, excluding Android | Pass; existing warnings retained |
| Complete render library tests | 63 pass |
| Thirteen affected render integration suites | 76 pass |
| Host failure boundaries, memory policy and preview memory | 17 pass |
| Desktop failure boundaries at default Info | 7 pass |
| Panel style consumer guard | Pass; no panel file changed |

This selection contains 163 distinct passing cases, zero failures and no ignored
cases. A subsequent test-only commit, `363120af83966ed2c166bebf92ccd1eff4bba71b`,
adds and passes one real GPU curve-binding regression without changing production
source. Together these are 164 distinct selected cases across two invocations,
not a claim that the full selection was rerun on the test-only commit. This is
Windows local evidence, not complete desktop or cross-platform acceptance.

| New case ID | Evidence and boundary |
| --- | --- |
| #61-A32 | Later scratch-slot refusal preserves old allocation identities, capacity and actual pixels; increasing the limit permits the real larger blur |
| #61-A33 | Late parameter-buffer refusal retains the typed capacity cause; recovery reuses admitted scratch and renders real grayscale pixels |
| #61-A34 | Curve-LUT refusal, queued writes and abandoned encoders retain destination capacity until actual submission and GPU completion |
| #61-A35 | Real packaged PNG refusal leaves resource counters unchanged; recovery uploads the existing attributed package image and renders opaque pixels |
| #61-A36 | Actual parameter refusal preserves the original reply and request/operation/phase context in default file/Error-stderr output without stdout leakage |
| #61-A37 | Three shrink/grow preflight cycles without intervening draws retain scratch identities, bind the new black-producing LUT instead of the old red one, and return to the same admitted capacity |

Logs: `E:/Dev/aem/artifacts/pr-queue/effect-admission-final-*.log` and
`E:/Dev/aem/artifacts/pr-queue/effect-curve-supplement.log`.
Each invocation has a source-bound exit receipt. Source/log hashes are retained
in `E:/Dev/aem/.tools/issue-closure-review/effect-admission-acceptance.json` and
`E:/Dev/aem/.tools/issue-closure-review/effect-curve-acceptance.json`.
Prior receipts remain attached to their originally tested source commits.

PR #395 subsequently passed its current Rust/GPU, Linux, Windows and macOS Intel
checks and merged as `58abfe01692e645e9234bdd686806fe8f05c6c8a` with a Chinese
merge message. The earlier Windows timeout failures remain historical failures;
the later passing run does not erase them. Integrating that main commit into this
follow-up changed ancestry only: the accepted source tree was identical before
and after the merge.

No dependency, asset, panel, existing assertion or pixel tolerance changed.
Remaining renderer resources, upload staging, shared session policy, platform
pressure, combined preview/export/lifecycle acceptance and #67 reuse are pending.
#60/#61/#67 remain open; precision settings and 3D integration remain parked.

## Sixth increment: video frame-blend parameters

The lazy 16-byte video frame-blend weight buffer now uses the same allocation
budget as its images. A refused buffer does not publish a partial pipeline or
acknowledge an unfinished cache image. Its original typed capacity cause retains
the object, source, PTS, slot, extent, phase and buffer descriptor context.
An already admitted, unready cache image remains available for a later retry;
this change does not claim complete operation rollback or preview recovery.

An encoder reservation is acquired before allocating or replacing the output,
tracking the destination or queuing the uniform write. Scope exhaustion therefore
preserves the completed output and cannot leave a new queued weight update.
Successful writes retain their destination through the existing upload journal,
and allocation scopes hold the weight alongside images through abandonment or
GPU completion. Texture-category subtotals continue to exclude this buffer;
combined capacity includes it once by identity. Queue-owned staging is still
outside this destination-capacity scope.

Prepared cases exercise an actual late buffer refusal with preserved decoded
pixels and real linear-blend recovery; scope exhaustion without a queued write
or false completion stamp; open and finished abandoned scopes plus teardown;
and the real refusal through unchanged replies and default file/Error-stderr
output with stdout isolation. No existing assertion or tolerance changes.
Record source-bound results separately before treating these cases as accepted.

The additional output-preservation regression reproduced the premature replacement
on test-only source `2277fc9bbaa648a798d2afd629f0c4acaa4c4cc6`: scope refusal had
already replaced allocation identity 2 with 8. The expected failing invocation is
retained in `E:/Dev/aem/artifacts/pr-queue/blend-output-before-fix.log`, with an
explicit exit-101 receipt. This is reproduction evidence, not a passing case.
The correction moves the same reservation before target allocation and keeps
the regression assertions unchanged. Final-source evidence follows.

## Accepted local evidence for the sixth increment

Initial source `d80d42b8bd5ca0084078b9f1ca871dd6818ae101` passed the complete
render library (67), seven affected GPU suites (36), host boundaries/policy/preview
(17), desktop default logging boundaries (8), workspace/all-targets FFmpeg check
and the panel style guard: 128 selected passing cases, zero failures or ignores.
That selection lacked the output-preservation regression; its green count did
not prove that a refused first mixture preserved the decoded output.

Test-only source `2277fc9bbaa648a798d2afd629f0c4acaa4c4cc6` then reproduced that
defect, with cargo exit 101 at the allocation-identity assertion. The correction
in `a4f44c58` kept the regression file unchanged and moved scope admission before
target allocation/replacement. Final checked source
`403b09f2f9bce75c9670c330f9b80ad59409bab3` also integrates actual main
`7967ae21d3ba89938568b50ecedfb704799ed016`, including PR #396's export clock and
native failure evidence changes and the merged exposure/effect increment.

| Final-source check | Result |
| --- | --- |
| Four frame-blend admission/output-preservation GPU library cases | 4 pass |
| Frame-blend and video-lifetime integration suites | 14 pass |
| Desktop failure boundaries at default Info | 8 pass |
| Workspace/all-targets FFmpeg check, excluding Android | Pass; existing warnings retained |
| Panel style consumer guard | Pass; no panel file changed |

These are 26 selected passes on the final source, zero failures or ignored cases.
The earlier 128-case selection remains evidence for its exact initial source;
it was not all rerun on the final source. The selections overlap and must not be
added. Neither selection is the complete desktop suite or cross-platform CI.
The worktree remained clean and unchanged during each sequential invocation.

| New case ID | Evidence and boundary |
| --- | --- |
| #61-A38 | A real late 16-byte weight refusal retains the typed required/budget cause, decoded stamp and actual black pixels; raising the limit permits cache completion and the real linear midpoint |
| #61-A39 | Exhausting 64 encoder scopes does not acknowledge an unfinished cache or queue a weight write; reclaiming then dropping scopes returns to the startup image without another submit/poll, and real blend recovery succeeds |
| #61-A40 | Open and finished abandoned scopes retain the new weight alongside images after actual GPU completion and working-set reclaim; dropping them releases to the startup image, then teardown with external device/queue handles releases to zero |
| #61-A41 | Actual weight refusal preserves the original reply and request/operation/object/source/PTS/slot/extent/phase/bytes/time in default file/Error-stderr output without stdout leakage |
| #61-A42 | The same regression that failed before the correction now preserves output allocation identity, decoded stamp, actual white pixels and budget state on scope refusal; a later actual mixture produces the linear midpoint |

Logs: `E:/Dev/aem/artifacts/pr-queue/blend-admission-final-*.log` (initial),
`blend-output-before-fix.log` (expected failing reproduction), and
`blend-output-final-*.log` (final). Each command has a source-bound exit receipt.
Source/log hashes and the unchanged-regression proof are retained in
`E:/Dev/aem/.tools/issue-closure-review/blend-admission-initial-acceptance.json`
and `E:/Dev/aem/.tools/issue-closure-review/blend-output-final-acceptance.json`.
The expected failing control is explicitly excluded from passing acceptance.

No dependency, asset, panel, existing assertion or pixel tolerance changed.
An unready new cache image can still remain after a first weight refusal; this
is not a complete operation rollback claim. Fixed and other dynamic renderer
resources, upload staging/raw external scope, shared session policy, platform
pressure, full preview/export/repeated lifecycle acceptance and #67 remain pending.
#60/#61/#67 remain open; precision settings and 3D integration remain parked.

## Seventh increment: fixed resources and lazy SDK instances

Builtin particle effects and presets were already retired. Their removal retained
SDK scene rendering, whose implementation still allocated a 3 MiB instance buffer
for every renderer. This increment removes that unconditional allocation without
restoring any retired effect. A nonempty SDK frame requests its actual instance
capacity, bounded by `MAX_SPRITES`; empty frames allocate nothing. Refused growth
preserves the old allocation, reuse needs no new capacity even below a lowered
limit, and working-set reclamation releases the buffer after outstanding users.

Fixed draw uniforms (32,768 bytes), planar geometry (1,310,720), clear vertices
(72), startup white (4), effect fallback white (4) and identity LUT (1,024) now
share admission: **1,344,592 descriptor bytes in six allocations** per renderer.
`FIXED_RENDERER_ALLOCATION_BYTES` describes this requested-limit baseline, not
physical VRAM, staging, optional timers or an entire frame peak. The independent
test pins both the byte sum and allocation count. Fixed reservations precede
queue uploads; each of the six startup refusals releases partial admission to
zero without an extra caller submission or poll. Upload destinations retain
their reservations through queue completion; bindings drop before their backing
resources. The mapped clear vertices do not add a queue write.

Existing low-budget fixtures now explicitly fund these six fixed allocations.
Their dynamic allowances, precise refusals, pixel assertions and tolerances are
preserved. The cross-queue fixture funds its second renderer before constructing
it. Expected combined byte totals and allocation counts intentionally changed;
this is additional enrolled capacity, not an assertion-removal workaround.

| New case ID | Accepted evidence and boundary |
| --- | --- |
| #61-A43 | Independently counted fixed descriptor bytes and six identities; real teardown with external device/queue handles retires to zero |
| #61-A44 | All six exact startup byte boundaries retain typed causes/context and release partial reservations immediately |
| #61-A45 | Open and finished abandoned commands retain fixed resources after renderer teardown and GPU completion; abandoning their actual owner releases them |
| #61-A46 | Empty SDK data allocates no instance buffer; exact-size allocation, refused growth with stable old identity, reuse below the limit, count bound, queued-write retention and recovery are exercised |
| #61-A47 | A fully culled SDK pass clears its real target without an instance allocation, then visible instances recover real nontransparent pixels |
| #61-A48 | Real startup refusal preserves its reply/cause and request/operation/phase/bytes/time in default Info file and Error-stderr output with stdout isolation |

The fixed-capacity control on `9cbb23b5` recorded only 4 bytes against the
independent 1,344,592-byte expectation (cargo exit 101). During this increment,
the first lazy implementation incorrectly rejected a zero-instance pass; test
source `bbe819a5` reproduced the missing-buffer error. Correction `64f9798e`
keeps the clear pass and skips its zero-instance draw. Neither control is a
passing acceptance case. The new recovery fixture subsequently needed four
setup/eligibility lines to produce visible instances; its transparent/visible
pixel assertions remain intact.

## Accepted local evidence for the seventh increment

| Source and selection | Result |
| --- | --- |
| `14a9e240`: complete render library | 69 pass |
| `da94f499`: nine completed, unaffected GPU integration binaries | 60 pass |
| `0a698095`: complete source/video allocation integration suites | 8 pass |
| `0a698095`: desktop default logging boundaries | 9 pass |
| `0a698095`: workspace/all-targets FFmpeg check, excluding Android | Pass; existing warnings retained |
| `0a698095`: panel style consumer guard | Pass; no panel file changed |

These are **146 distinct selected passing cases across source-bound runs**;
they were not all rerun on the last commit. Only two integration fixture files
changed after the 69-case library run; production and library-test sources are
unchanged. Only `source_allocation_budget.rs` changed after the nine passing GPU
binaries. That multi-binary invocation later failed two source-budget cases
because three expected capacity expressions omitted the newly enrolled fixed
baseline. Its overall exit remains 101; it is not labelled green. Correcting
those three fixture expressions and rerunning the entire source/video allocation
suites produced the eight passing cases above. Earlier cross-queue funding and
new recovery-fixture failures are retained too; later success does not erase them.

Logs are `E:/Dev/aem/artifacts/pr-queue/fixed-admission-before-fix.log`,
`empty-instance-before-fix.log`, `fixed-admission-*`, `fixed-final-*`,
`fixed-complete-*` and `fixed-finish-*`. Source-bound exit receipts retain the
failed invocations as well as successes. Source/log hashes, unchanged production
proof and retained empty/visible pixel assertions are recorded in
`E:/Dev/aem/.tools/issue-closure-review/fixed-admission-acceptance.json`.
Worktrees stayed clean and unchanged during each sequential validation command.
This selection is not a complete desktop suite or cross-platform CI claim.

PR #398 passed all four required current checks and merged as
`50d43ee33b53f0b9ffbbcdf13d42e3b243716c02` with a Chinese merge message;
the seventh increment includes that main ancestry. No dependency, asset, panel,
layout, style or interaction changes. Allocation timing and the zero-instance
clear correction are the functional resource changes described above.

Optional GPU timers, remaining dynamic renderer resources, staging/raw external
scope, shared session/platform policy, bounded preview/export recovery and full
repeated lifecycle acceptance remain pending. #60/#61/#67 remain open; #67 reuse,
project precision and 3D integration are not completed by this increment.

## Eighth increment: optional GPU timing buffer admission

`GpuTimer::new` now uses its renderer's allocation budget. A supported timer
admits a 1,024-byte resolve buffer and four 32-byte readback buffers: **1,152
descriptor bytes in five allocations**, independent of query-set/driver memory.
It returns `Result<Option<GpuTimer>>`: unsupported timestamp queries remain a
normal `Ok(None)`, while capacity refusal retains the original typed rejection.
All fallible buffer reservations precede publication and queue writes are not
needed. Partial construction therefore retires directly when no command scope
owns those reservations. The host propagates refusal instead of reporting
unsupported timing; a refused profiling replacement preserves the old timer.

Pending readback callbacks own both their actual raw buffer and its allocation
lease. Dropping a timer releases its other buffers, while the pending 32 bytes
retire only when the actual mapping callback runs. No frame-loop wait, retry,
new dependency, retired particle product, panel or style is introduced.

| Item | Evidence | Disposition |
| --- | --- | --- |
| #61-A49 | Independent sum of four aligned resolve slots and four timestamp readbacks; timer and renderer teardown | New admission; expected failing control reproduced |
| #61-A50 | All five exact capacity boundaries reject with original scope/required/budget/label; partial construction retires before poll/submit; recovery succeeds | New refusal and cleanup coverage |
| #61-A51 | A device without enabled timestamp queries returns `Ok(None)` and leaves admission unchanged | Normal unsupported branch |
| #61-A52 | Refused replacement preserves old capacity and slot reservations; existing slots work below a lowered limit | New preservation coverage |
| #61-A53 | Actual resolve/copy commands retain timing capacity through abandoned and submitted command owners | New lifetime coverage |
| #61-A54 | Actual queued timestamp words, pending map, timer drop, and real successful callback; only the mapped 32 bytes remain until callback | New asynchronous lifetime coverage |
| #61-A55 | Actual final timing-buffer refusal through existing `respond`, with request 207/operation/timing/original typed cause; default file, Error stderr and stdout isolation | Existing error boundary, newly exercised failure |

The failing control at `3a98fa9d` reported only 1,344,592 admitted bytes against
1,345,744 independently expected bytes. Cargo exited 101 after an actual GPU
assertion failure, not a compile failure. It is not a passing acceptance result.
The subsequent API adaptation preserves the original byte/count/teardown
assertions; existing preview pixel and timing assertions are retained.

Source-bound acceptance comprises **165 distinct selected passing tests**:
- source `c2c1e26e`: render library 70, four GPU integration suites 16, host
  library 69; the selected existing desktop boundary suite also passed nine
  cases at this source (not counted twice)
- source `c453aa53`: the ten-case desktop boundary suite including #61-A55,
  workspace/all-targets FFmpeg check (excluding retired Android), and the panel
  style guard all pass

The only source delta between these selections adds the desktop test module and
its registration. Production and the accepted render/host test sources are
unchanged. Existing assertions and preview pixel tolerances are retained. These
are selected tests, not a claim that the entire workspace test matrix ran.
Raw logs and per-command source/exit receipts are `artifacts/pr-queue/timing-
admission-*.log` / `.exit.json`; `timing-admission-acceptance.json` in the local
review directory verifies source deltas, log SHA-256, counts and the failing
control. Builds, temp data and artifacts remained on E:.

Query-set metadata, driver allocations, raw external encoders, and other dynamic
layer/vector/mask/compositing resources are not fully covered by this increment.
Shared session policy, platform pressure, per-frame peaks, bounded full preview
and export behavior, repeated lifecycle acceptance and #67 remain pending.
This does not complete #60/#61/#67; precision and 3D remain parked.


### Integration with the completed-frame retirement change

PR #399's original commits are included, without reimplementing its ownership /
work-retention separation. Its AR-01 regression still funded only the former
four-byte renderer baseline. Direct composition at `f8b467bc` reproduced an
actual startup refusal (`required_bytes=32772`, `budget_bytes=2052`), before
entering the retirement loop; cargo exit 101 is retained as a failed control.
`1db71805` changes exactly four fixture lines: import the independently tested
fixed-capacity constant and use it in the allowance and two exact retirement
expectations. The target allowance, four-frame loop and strict requirement that
completed transient targets leave only startup capacity are unchanged. This is
an accounting-scope fixture update, not a relaxed lifetime assertion or a new
product defect fix. The other incoming retirement assertion remains tightened.

The combined source `1db71805` passes **174 distinct selected tests**: render
library 70, seven GPU suites 25 (including AR-01), host library 69, and desktop
default-log boundary 10. Workspace/all-targets FFmpeg check (excluding retired
Android) and the panel style guard also pass. Raw `timing-admission-combined-*`
logs/exit receipts and `timing-combined-acceptance.json` bind these results to
the exact source and preserve the failing control. This is the final local
combined selection, not an assertion that all workspace tests or the updated
public CI passed. No tracked source changed while either validation ran.

This integrates the incoming resource-lifetime implementation and keeps the
seven new timing cases in the same PR as the fixed-resource work. #61's remaining
dynamic resources, shared/platform policy, preview/export/lifecycle acceptance
and #67 are still pending; retired builtin particle products remain absent.


### Optional timestamp capability in desktop CI

Public source `d8671a3e` passed Rust/GPU, Linux and Windows, but macOS Intel
job119245623228 failed the desktop timer fixture: its adapter returned the
documented normal `Ok(None)`, while the fixture required a budget error from
every adapter. This is a capability-precondition defect in the test. Production
support detection and budget handling are unchanged.

Source `f6f7698e` checks actual enabled timestamp features and the native queue
period before selecting the oracle. Supported devices still require the
original exact typed refusal, byte counts, reply/cause/context and all default
file/stderr/stdout assertions. Unsupported devices must return `None`, preserve
the entire admission snapshot, leave GPU health intact, and emit no command
failure on any output channel. A second test deliberately disables timestamp
requests on every platform, so this normal branch is exercised independently
of CI hardware. No test is ignored or retried, and no production API changes.

Both selected desktop subprocess tests pass at exact source `f6f7698e`, with
zero failures or ignores. `timing-unsupported-fixture.log` / `.exit.json` and
`timing-unsupported-acceptance.json` retain source, command, exit and log hash.
The earlier174-case combined acceptance remains attached to its original
source; these two executions are not presented as another full combined run.
Fresh public macOS CI is still required before merging. A successful normal
unsupported test does not prove native timer refusal on that adapter.


## Ninth increment: vector destinations and asynchronous upload staging

At source `e994f343`, vector vertex and group-opacity buffers, plus their
mapped staging pool, reserve their actual rounded descriptor capacities from
the renderer's combined admission budget. Growth reserves before replacing
the old buffer; refusal preserves its aliases and completed output. Cached
capacity remains reusable below a newly lowered limit. Cached opacity bindings
retain the buffer allocation for their own lifetime.

Encoded copies retain the destination as work, without promoting it back to an
actual resource owner. Pending staging/remap callbacks retain their admitted
buffers through pool replacement or teardown. This preserves #399's distinction
between an actual owner and work retained by an older frame. Upload allocation
errors preserve typed rejection and layer/frame/phase context through the
existing error boundary. Healthy Present behavior and pixel semantics remain.

| Case | Actual path and oracle | Disposition |
| --- | --- | --- |
| #61-A56 | 24-byte destination and staging each round to32; exact64-byte increase and teardown | New capacity coverage |
| #61-A57 | Refused growth preserves identity and aliases; successful replacement and below-limit reuse | New preservation coverage |
| #61-A58 | Staging refusal leaves counters and destination unchanged; real copied bytes before and after cached reuse below limit | New failure/recovery coverage |
| #61-A59 | Unfinished and finished raw upload commands retain both buffers after caches/pool drop, until command abandonment | New upload-owner coverage |
| #61-A60 | Actual submission/remap completion with overlapping allocation scopes; completed destinations are not renewed by unrelated frames | New retirement coverage |
| #61-A61 | Nested vector opacity, real pixels, exact vertex/opacity/staging sums, reuse and full clear/drop retirement | New composition coverage |
| #61-A62 | Real reupload budget refusal preserves exact prior output; restored budget produces the changed alpha | New output-preservation coverage |
| #61-A63 | Actual vector failure through `respond`, request208/layer77/frame0/phase/cause/elapsed; default file, Error stderr and stdout isolation, then actual recovery | Existing boundary, newly exercised failure |

The native GPU control at `30dfdce8` failed with 1,344,592 admitted bytes
against 1,344,656 expected bytes, cargo exit101. It is retained as a failing
control, not a passing result. Its three original capacity/count/teardown
assertions are unchanged. Existing native remap-failure assertions still
require the same `Remap` cause, now nested in the existing `RenderError` wrapper;
existing pixel tolerances and return/protocol behavior are unchanged.

Exact source `e994f343` passes **192 distinct selected tests**: render library75,
seven GPU suites37, host library69, desktop default-log boundary11, with zero
failures or ignores. Workspace/all-targets FFmpeg check (excluding retired
Android) and the panel style guard also pass. `vector-admission-*.log` and
`.exit.json` records bind commands and exits to the source; the local
`vector-admission-acceptance.json` records their SHA-256 and verifies the failing
control and retained assertions. No tracked source changed during validation.
All build caches, temporary data and artifacts remained on E:.

This selection does not claim full workspace/public CI acceptance. Group raster
textures, mask/compositing resources, other external/raw allocation boundaries,
shared session policy, platform pressure, frame peaks, bounded preview/export
and repeated lifecycle acceptance remain pending. #61 and #67 remain open;
precision and 3D remain parked. No panels, styles, dependencies or assets changed.


## Tenth increment: mask mesh and filter uniform admission

Production source `64fda9ba` admits cached mask triangle buffers and every624-byte
feather/combine uniform from the renderer's combined budget. Mapped mesh
initialization preserves the previous upload behavior and reserves before
replacement. Uniform queue writes share the existing pending-upload tracker;
their capacity survives the cache owner until actual completion. Errors retain
the original cause and layer/mask/frame/phase through the existing boundary.
Cached bindings, shader math, return/protocol semantics and pixels are retained.

| Case | Actual path and oracle | Disposition |
| --- | --- | --- |
| #61-A64 | One144-byte quad mesh plus three624-byte uniforms; exact2016-byte increase, pixels and retirement | New capacity coverage |
| #61-A65 | Real refusal at each of three uniform cut points; exact partial capacity, no successful fingerprint, existing slots reused and actual recovered pixels | New failure/recovery coverage |
| #61-A66 | Refused mesh replacement preserves original identity, fingerprint and exact pixels; normal cache reuse below limit; restored budget changes actual pixels | New preservation coverage |
| #61-A67 | Actual queued uniform write survives cache drop until native completion; a later encoder does not renew retired ownership | New asynchronous lifetime coverage |
| #61-A68 | Both unfinished and finished mask command owners retain all buffers after cache drop and GPU polling; unrelated later scopes do not extend old work | New abandonment/retirement coverage |
| #61-A69 | Actual mask budget refusal, request209/layer77/mask55/frame0/phase/cause/elapsed, unchanged reply and recovered pixels; default file, Error stderr and stdout isolation | Existing boundary, newly exercised failure |

The native control `ecff3b68` reproduced 1,344,592 admitted bytes against
1,346,608 expected bytes and cargo exit101. It is not counted as a pass. Its
capacity and pixel assertions remain unchanged. The new fixture's initial
teardown oracle incorrectly compared the cumulative peak with the preallocation
snapshot. The corrected full retired snapshot retains the independently counted
2016-byte peak while requiring every live byte/count/scope to retire; this is a
fixture correction to peak semantics, not a relaxed capacity or pixel check.
Existing mask reuse tests retain their binding identities and pixel tolerances;
their fallible helper calls now require successful results explicitly.

After integrating main #401's original error contracts and the PR400 timestamp
capability correction, source `64fda9ba` passes render library86, eight actual GPU
suites40, host library69 and workspace/all-targets FFmpeg check (excluding retired
Android). Its new desktop fixture failed an unchanged transparent-outside pixel
assertion because the test project still had the normal opaque background.
`584b37c4` adds exactly one fixture line selecting a transparent project background;
every assertion and all production/render/host sources are unchanged. The final
desktop default-channel suite14 and style guard pass on this source. Together
these are **209 distinct selected passing tests**, with zero failures or ignores
in the accepted selections; the first failed desktop run is retained separately
and its13 passes are not added again. The source/fixture delta is verified exactly.
`mask-admission-combined-*.log` / `.exit.json` and the local
`mask-admission-acceptance.json` bind source, command, exit, counts, log SHA-256
and the failed controls. The original vector192-case result remains attached to
`e994f343`; the combined209 selection supersedes it for integration. These are
selected runs, not full workspace/public CI acceptance. No tracked source
changed during either validation; all caches, temp data and artifacts stayed on E:.

PR400's exact combined source `6042cd04` separately passes190 selected cases
(render76/GPU33/host69/desktop12) and workspace FFmpeg check after resolving main
#401's two conflicts. The builder and alpha initialization still precede GPU
queue writes and preserve native error identity. Its public run39614881573 is
tracked separately; local acceptance does not assert a public green result.

Final publication: all four required public checks passed on `6042cd04` in
run39614881573, including macOS Intel. PR400 was then squash-merged as
`d60aaea2` with a Chinese commit message. Integrating that squash into this
batch preserves the accepted source tree exactly; only this completion record
differs from the final local source bundle. Vector/mask public CI is a separate
publication stage and is not inferred from PR400's green checks.

Mask `clear` retains the existing warm uniform slots; this increment observes
them but does not add cold pressure eviction. Mask/vector/group/effect/matte
textures, compositor and presenter buffers, other external boundaries, shared
session/platform policy, frame-class peaks, bounded preview/export and repeated
lifecycle acceptance remain pending. #61/#67 stay open; precision and 3D remain
parked. No panel, UI, style, asset or dependency changed.


## Eleventh increment: compositing system admission

Source `68f0de50` admits the full aligned matte-extraction and layer-transfer
uniform buffers, plus the default R8 zero-coverage texel, from the renderer's
shared budget. Native construction completes every reservation before the first
queue write, and publishes the compositor/build count only after success.
Both ordinary and exposure-local callers retain the native refusal through
frame/phase context. Both uniform write sites and the zero upload retain their
destinations through the existing upload tracker. Cached groups drop before
their resources. Shader math, pixels, cache policy and UI behavior are retained.

| Case | Actual path and oracle | Disposition |
| --- | --- | --- |
| #61-A70 | Real buffer sizes plus one texel: 131073-byte increase on this device; three allocations and final retirement | New capacity coverage |
| #61-A71 | Native refusal at each of three construction cut points; exact required/limit bytes, immediate partial cleanup, no published compositor/build count, recovery | New refusal/recovery coverage |
| #61-A72 | Ordinary matte failure retains old output pixels and expected center alpha; restored budget reproduces exact pixels | New preservation coverage |
| #61-A73 | Exposure-local blending refuses initialization while retaining receiver inputs and output; exact recovered pixels | New independent caller coverage |
| #61-A74 | Real zero, extract and blend queue writes retain all three destinations after cache drop; actual completion retires them without renewal by a later encoder | New asynchronous lifetime coverage |
| #61-A75 | Unfinished and finished native clear commands retain resources after GPU polling; dropping their own work releases capacity despite an unrelated later scope | New abandonment/retirement coverage |
| #61-A76 | Real compositor budget failure through the existing command boundary, request210/frame0/phase/cause/elapsed, unchanged reply and actual recovered matte pixels; default file, Error stderr and stdout isolation | Existing boundary, newly exercised failure |

Native control `0eb42339` failed at 1344592 admitted bytes versus1475665 expected,
cargo exit101, before implementation. Its four capacity/count/retirement
assertions remain unchanged. The first implementation compile failed borrow
checks; the retained run is not an acceptance pass. The correction narrows the
frame borrow and clones the shared test budget handle, without changing any
assertion. The six initial compositor GPU cases passed before broader checks.

Exact source `68f0de50` passes **235 distinct selected tests**: render library92,
ten real GPU suites59, host library69 and desktop default-log boundary15,
with no failures or ignores in these accepted runs. Workspace/all-targets FFmpeg
check (excluding retired Android) and the panel style guard pass. These are
selected runs, not full workspace or public CI acceptance. Logs and exit
receipts `compositing-admission-final-*` bind commands to the frozen source;
the local `compositing-admission-acceptance.json` verifies counts, failed control,
retained assertions and log SHA-256. No source changed during validation; all
build caches, temporary data and artifacts remained on E:.

This measures enrolled logical descriptor capacity, not driver VRAM or queue
staging. Effect scratch already uses admitted replacement-before-commit and is
existing coverage. Remaining compositor/mask/vector/retained-layer textures, presenter buffers,
external/raw boundaries, shared session/platform pressure, frame-class peaks,
bounded preview/export policy and repeated lifecycle acceptance remain pending.
An existing foreign `perf/presenter-source-reuse` worktree has uncommitted
presenter/session changes; no presenter work was started here to avoid overlap.
#61/#67 stay open; precision and 3D remain parked. No panels, styles, dependencies
or assets changed.


## PR404 native-window validation followup

Public run39620361094 on vector/mask head064aa623 passed Rust/GPU, Ubuntu and
macOS Intel. Windows failed two existing native-window tests: paced sampling
timed out awaiting Present (in-flight11.5s, queue0), and path selection timed out
awaiting capture_ui. Its731 passes/two failures/one pre-existing ignore are not
treated as successful acceptance. Raw job logs, failure.json and default file
logs are retained in the local pr404-windows-failure evidence directory.

The path driver waited for both presentation and screenshot replies on the
native window owner without message dispatch. Sourcea1a2b8da switches those
test waits to the existing native message pump and uses it for sampling's final
screenshot as well. Presentation keeps its10-second wait and screenshots keep
the existing60-second wait. Every original assertion in all three changed test
files is preserved byte-for-byte. Product code and interaction remain unchanged.

Exact sourcea1a2b8da passes three selected real Windows controls: cross-thread
native messages with original deadline assertions, complete main/floating path
routes and screenshots, and paced sampling across all four modes. Source-bound
native-capture logs/exit receipts and native-capture-acceptance.json record the
commands, original failure and SHA-256. These are separate from compositor
source68f0de50's235 selected passes; they are not a full workspace or a single
combined-source run. The integration retains the accepted production code and
identical tested native drivers. Sampling already pumped messages while awaiting
Present, so the remote baseline timeout's precise cause and resolution remain
unproven until further public/native evidence. No assertions, retries, ignores,
timeouts or product semantics were relaxed to obtain a green result.


## PR404 main integration validation

Source `8ead9fbc` integrates main `164baf37`, including its dependency-audit
updates and complete native attachment message dispatch. The only path-driver
conflict keeps main's 30-second attach pump and all original assertions, while
using the shared 60-second capture helper. Renderer/host source remains identical
to accepted `68f0de50`; core/effects and the lockfile match the integrated main.
No dependency is added by this resource-admission increment.

The frozen integrated source passes workspace/all-targets FFmpeg check, core17
and effects7 library cases, all15 default-log composition boundary cases, and
the three actual Windows message/path/playback controls: **42 selected passes**.
The core library also retains one pre-existing ignored manual large-document
history measurement; that case was not run or counted as a pass. These results
are separate from the earlier235 compositor selections and do not represent a
full workspace test run. Source-bound `compositing-admission-parent-main-*`
logs and `parent-main-acceptance.json` retain exits, counts and SHA-256.

This resolves the main-integration conflict but does not establish the cause of
the earlier remote sampling Present timeout. Public checks must be evaluated on
the newly published head. No assertions, waiting limits, ignores, product panel
code or UI behavior were changed by conflict resolution. #61/#67 remain open.


## Twelfth increment: matte and transfer working-texture admission

Source `d04ef55a`, integrated with main `776c1b31`, admits R8 matte targets and the transfer source in its actual
target format from the shared renderer budget. Category checks and every new
working-target reservation finish before replacing any live matte/source,
dimensions or cached transfer bindings. Partial failure drops only the prepared
replacement set. Both ordinary and exposure-local callers retain frame/phase,
layer/luma/size and the original typed refusal. Cached transfer groups retain
source and accumulator admission aliases independently of cache owners; command
scopes retain their own work through actual completion or abandonment.

| Case | Actual path and oracle | Disposition |
| --- | --- | --- |
| #61-A77 | Two4x4 R8 mattes plus4x4 RGBA8 source:96 actual bytes, three allocations and final release | New capacity coverage |
| #61-A78 | Refusal at both replacement mattes and final source, plus original category checks; native identities/bindings/pixels unchanged, warm reuse below lowered limit and successful resize | New atomic replacement coverage |
| #61-A79 | Cached groups render real Screen red/blue pixels after source and admitted accumulator owners drop; capacity retires with the bindings | New independent binding-lifetime coverage |
| #61-A80 | Unfinished, finished-abandoned and actually submitted clear commands preserve working targets until their own retirement despite unrelated later scopes | New command-lifetime coverage |
| #61-A81 | Ordinary matte and exposure-local transfer refusal preserves prior output; restored budget reproduces exact expected pixels without rebuilding pipelines | New independent caller coverage |
| #61-A82 | Real resized-matte refusal through request211 with original reply/cause/frame/phase/elapsed, preserved pixels and recovery; default file, Error stderr exactly once and stdout isolation | Existing error boundary, newly exercised failure |

Native control `837df6bc` failed at1475665 admitted bytes versus1475761 expected,
cargo exit101: the missing96 bytes are two mattes and one transfer source. Its
five capacity/count/retirement assertions remain unchanged; only setup follows
the combined atomic preparation API. First implementation source `6bc150e8`
passed all five new cases but failed one old system-upload fixture: its total
also included20480 newly admitted working-texture bytes that legitimately retired
when cache owners dropped. The fixture now explicitly holds those working
owners outside its three-system-resource observation. All old assertions remain
byte-for-byte unchanged; separate new lifetime tests exercise working retirement.
That actual failure log remains retained and excluded from acceptance.

The first desktop fixture also caught an earlier scratch-slot refusal with the
same4096-byte delta. Its strict matte-phase assertion failed correctly. The
fixture now preflights the larger scratch set before restricting capacity;
all original error, channel and pixel assertions remain unchanged. Its15 passes
and one failure are retained separately. A subsequent run terminated before
test output with exit1073807364; that interruption is retained and not counted.
The resumed source `4d129c4d` still refused at the preceding growing source
snapshot, correctly failing the matte-phase assertion. The final fixture keeps
source geometry at16x16 while resizing only the composition/matte framebuffer;
its original center, edge, byte-count, phase and channel assertions are intact.
The failed15-pass/one-failure run is retained as evidence, not acceptance.
That preceding same-frame input-snapshot allocation still needs its own layer,
extent and phase context; it is a documented pending observation, not a fixed
site in this increment.

The exact frozen final source passes **241 distinct selected tests**: complete
render library97, ten real GPU suites59, host library69 and default desktop
composition/error boundary16. Workspace/all-targets FFmpeg check excluding
retired Android and the panel style guard pass. Accepted runs have no failures
or ignores. The earlier97 library passes on `6de3387b` are not added again.
Main's integrated model/path changes prompted the final source-bound matrix.
`matte-admission-main-*` logs/exit receipts and
`matte-admission-acceptance.json` bind commands, native failures and SHA-256 to
the exact source. These are selected runs, not full workspace or public CI.
No source changed during validation; all own caches, temporary data and artifacts
remain on E:. No panels, styles, assets or dependencies were changed.

This closes admission/lifetime coverage for these working targets, not #61's
entire policy. Layer accumulators, vector/mask/group/retained-layer textures,
presenter buffers (foreign worktree overlap), external boundaries, shared
session/platform pressure, frame peaks, bounded preview/export and repeated
lifecycle acceptance remain pending. Native effect scratch was already admitted.
#61/#67 stay open; precision and 3D remain parked.


## Thirteenth increment: transactional layer accumulators and input-snapshot context

Accepted product/test source: `758eae7c0b6981c5cb2bc643cbf6734937c12caf`.
This increment follows the twelfth increment's matte/transfer working textures;
it does not close #61 or #67.

Both actual layer accumulator targets now share the existing AllocationBudget.
Replacement reserves both targets before replacing either native owner or its
dimensions. Refusal at either slot drops staged allocations and retains the old
pair; the ordinary and retained-local compositing callers preserve the original
typed rejection with frame, phase, slot, extent and format. Warm reuse needs no
new capacity. Existing command scopes and cached-binding aliases keep admission
until the resource's own work retires, including abandoned encoders/commands.

The native test-only control `28ecd74b` reported a 128-byte shortfall for two
actual 4x4 RGBA8 targets and exited 101. Its original capacity, live-count and
release assertions remain unchanged. Initial implementation `2fce07b1` passed 101
library cases and failed the new readback fixture: it copied into a product
capture texture without COPY_DST. Source `c900273d` uses a padded native test
readback buffer instead, with every budget/dimension/pixel assertion retained.
The initial failure and corrected102-library/one-default preliminary runs remain
separate evidence; duplicate cases are not added to final acceptance.

Input snapshots were already admitted, but allocation refusals lacked frame,
layer, extent and phase. A separate native control `cee07538` reproduced the
real 4096-byte refusal with the original typed cause and failed the strict
context assertion. Snapshot allocation and source-category refusals now retain
those identifiers and their distinct original rejection scopes; the caller adds
the sampled frame. No return value, retry, deadline or allocation policy changes
for snapshots. The earlier `d266cfe9` fixture had no actual snapshot dependency
and failed lookup before the intended control; it is retained as an invalid
fixture result, not a demonstrated product defect. Its corrected matte-dependent
setup retains all assertions.

| Item | Native evidence | Disposition |
| --- | --- | --- |
| #61-A83 | Both actual accumulator descriptors contribute 128 bytes and two live allocations, then release exactly | New admission coverage |
| #61-A84 | Both replacement cut points retain old native identities, dimensions, red/blue pixels and booked capacity; warm reuse and successful resize | New atomic replacement coverage |
| #61-A85 | Encoding, finished-abandoned and actually submitted work keep both targets until their own retirement despite a later scope | New lifetime coverage |
| #61-A86 | Actual cached Screen binding still renders red/blue to magenta after accumulator/source owners are removed | New retained-binding coverage |
| #61-A87 | Ordinary and retained-local refusal retain target pixels and original phases; recovery reproduces gray 43 +/-3 and alpha 255 | New public rendering coverage |
| #61-A88 | Real resized accumulator refusal request 212 retains bytes, prior pixels, cause, elapsed time; default Info file and Error stderr exactly once, stdout isolated | New default-output observation |
| #61-A89 | Actual combined and source-category snapshot refusals retain layer 77, extent 32x32, original typed scopes, old native texture and pixels; successful recovery | New error-context observation |
| #61-A90 | Real snapshot refusal request 213 retains frame 0, layer 77, extent, phase, cause and elapsed time across reply, default Info file/Error stderr, with stdout isolation and recovery pixels | New default-output observation |

Final acceptance on the same frozen source: 103 render-library cases, 18 desktop
composition/default-output cases, 59 cases across ten selected GPU integration
suites, and 69 host-library cases: **249 distinct selected cases passed**, with
zero failures or ignores. Workspace/all-targets check with FFmpeg, excluding
Android, and the existing panel style consumer guard also passed. These are
selected local runs, not full workspace tests or public CI. No panels, UI,
dependencies or assets changed.

Raw logs and exact-source exit receipts are
`artifacts/pr-queue/accumulator-admission-final-{library,desktop,gpu,host,workspace,styles}.*`.
Controls and fixture failures are retained separately; the acceptance receipt
`.tools/issue-closure-review/accumulator-admission-acceptance.json` records hashes,
sources, counts and exclusions. Logical descriptor capacities do not measure
driver VRAM. Presenter ownership integration, remaining raw retained/group
resources, platform pressure queries, full bounded-preview policy and interval
scratch reuse remain pending. No whole-issue completion or blanket3D readiness
is claimed.


### PR407 native CI followup: expression authoring fixture

Public head `bab73092`, run `39700286732`, Rust/GPU job `119567585980`
failed the existing nested fractional-frame evaluator integration case.
The original error reported a real expression budget rejection in leaf frame
9.2, propagated through child frame 22.5. This identifies a budget refusal;
the precise runner scheduling/preemption cause remains unproven. It failed
before matte target allocation and is not a demonstrated matte admission defect.

The fixture directly installed PropertyExpression and entered QuickJS for the
first time inside the existing whole-frame deadline. It now installs the same
expression through production Engine/InComposition/SetExpression, which compiles
before creating its evaluation deadline. The original recorded evaluator,
fractional frames, result and input-preservation assertions remain unchanged.
Production runtime code, 20ms expression and 100ms frame allowances, cancellation
and failure semantics are unchanged. No retry, ignore or deadline extension was
added. Cold project loading may still encounter the existing wall-clock budget;
that separate limit is not claimed fixed by this authoring-path fixture.

Combined source `8d73d93e752a03630f4bc92db58c6ea348a34423` differs from accepted product source `758eae7c`
only in this integration fixture and the completion document. On that source,
both real-evaluator integration cases and 16 expression/sandbox
cases passed, with zero failures or ignores. The sandbox selection retains
interruption, failed-source recovery, deferred compile context and cancellation
checks. These are additional selected runs; they do not replace the 249-case
resource admission regression or claim a new full-workspace test pass.

The original public failure, exact-source exit records and hashes are retained
in `artifacts/pr-queue/pr407-rust-gpu-failed-job.log`,
`accumulator-admission-ci-{evaluator,expressions}.*`, and
`.tools/issue-closure-review/combined-evaluator-fixture-acceptance.json`.


## Fourteenth increment: retained receiver images and projected mattes

Accepted product/test source: `ff153bf25cbe037471205930e517f7a0de96ce48`.
The three native retained-receiver factories now use the existing shared
AllocationBudget: constant 1x1 source, full local image and projected R8 matte.
Each actual descriptor acquires admission independently of the existing exposure
category reservation. Partial failure rolls back both staged native admission
and category capacity. Original typed rejection is retained with bounded
composition ID, sampled frame, receiver ID, native extent, format and phase.
No rendering format, retry, deadline, project state or UI behavior changes.

Native control `0629bd97` reproduced four missing bytes for an actual 1x1
receiver: admitted1344592 versus expected1344596, cargo101. Its complete original
test and all capacity/live-count/release assertions remain unchanged. The initial
five-case run `4a06972c` passed three and failed two newly added fixtures. Drawing
retained coverage released materialization working mattes, so the intended local
copy refusal occurred at an earlier matte allocation. The corrected fixture
rewarms that independent stage before setting the cut point. One newly authored
error-string expectation incorrectly said Unorm; the existing native contract is
Rgba8UnormSrgb. Source `673bffe6` corrects that expectation and adds a real native
format assertion. Preexisting repository assertions and all capacity, pixel,
lifetime and recovery assertions are preserved. The two original fixture failures
and the subsequent five-case preliminary pass remain separate raw evidence.

| Item | Native evidence | Disposition |
| --- | --- | --- |
| #61-A91 | Actual 1x1 receiver contributes four shared bytes and one allocation; owner retirement releases exactly | New admission coverage |
| #61-A92 | Local32x32 RGBA image and projected64x64 R8 matte cut points preserve both budgets, old green/transparent pixels and exact recovery | New rollback and recovery coverage |
| #61-A93 | Two real renderers share the same allowance; refusal restores the category counter; releasing the first receiver enables the second | New shared-capacity coverage |
| #61-A94 | Open, finished-abandoned and submitted native copy work retain both8192 bytes until their own retirement, despite a later scope | New command lifetime coverage |
| #61-A95 | Retained native receiver survives Renderer teardown with four bytes/one allocation held until its own drop | New owner lifetime coverage |
| #61-A96 | Actual four-byte refusal request214 preserves typed cause, prior pixels and recovery; default Info file and Error stderr, stdout isolation | New default-output observation |

On the same frozen source, 108 render-library cases,19 desktop composition/error
cases,61 cases across11 GPU integration suites and69 host-library cases passed:
**257 distinct selected cases**, zero failures or ignores. Workspace/all-targets
FFmpeg check excluding Android and the existing panel style consumer guard also
passed. Preliminary reruns are not added to this count. The desktop observation
case exercises a real native refusal through CURRENT_COMMAND/respond; it is not
a full worker-dispatch integration case. These local selected runs do not claim
full workspace tests or public CI. Final publication may remove the extra terminal
blank line in the new native test file; its remaining bytes and all tested logic
must match this accepted source. No panels, styles, dependencies or assets changed.

Raw logs and exact-source receipts are
`artifacts/pr-queue/retained-raster-admission-final-{library,desktop,gpu,host,workspace,styles}.*`.
The before-fix/native-first/native-corrected logs preserve the failed control and
fixture history. `.tools/issue-closure-review/retained-raster-admission-acceptance.json`
records their hashes and exclusions. Logical bytes are not measured physical
driver residency. Remaining group/raw resources, foreign Presenter integration,
shared session/platform pressure, complete bounded preview/export and repeated
lifecycle acceptance remain pending. #60/#61/#67 stay open; precision and3D are
not declared ready by this increment.


## Fifteenth increment: native mask output and scratch admission

Accepted product/test source: `5e13bff8c9bb2113041d28f3193b2d318cf54d2a`.
This extends the earlier mask buffer admission to actual R8 image allocations.
The complete replacement scratch pool and all resized mask outputs are staged
before replacing native owners, dimensions or cached inputs. Any texture-stage
refusal rolls back the staged shared reservations and leaves the previous
targets, source-category accounting and bindings intact. Cached input/source
bindings retain the same allocation leases. Ordinary rendering still uses the
existing formats, mask parameters and typed failure paths, without retries,
deadline changes, project changes or silent effect removal.

The native control `09a2f9e4` reproduced 12,288 missing shared bytes for two 64x64
R8 scratch textures and one 64x64 output: booked 1,346,608 versus expected 1,358,896,
cargo exit 101. Its complete original capacity, live-count, submitted pixel and release
control is retained unchanged. Initial implementation `5b3c9809` passed all 13
mask library cases, including five new native cases. Older buffer-only tests
now pre-admit and explicitly retain actual native texture aliases before
measuring buffer deltas. Their existing assertion expressions remain unchanged;
the new cases independently verify complete texture capacity and retirement.
The existing desktop vertex-refusal fixture first warms the newly admitted
earlier texture stages, then changes an actual mask vertex to retain its original
buffer refusal, cause and recovery assertions.

| Item | Native evidence | Disposition |
| --- | --- | --- |
| #61-A97 | Actual output and two scratch descriptors add 12,288 bytes and three native allocations in addition to the original four buffers; submitted pixels and exact release | New native admission coverage |
| #61-A98 | Both scratch slots and output refusal preserve old native IDs, dimensions, bindings, source bytes, shared live counts and pixels; warm reuse and successful 128x64 recovery | New transactional replacement coverage |
| #61-A99 | A real cached Gaussian input still renders center 255 after cache owners clear; 4096 bytes remain admitted until that binding retires | New cached-alias lifetime coverage |
| #61-A100 | Encoding, finished-abandoned and actually submitted native mask commands retain all texture/buffer capacity until their own retirement despite a later scope | New native command lifetime coverage |
| #61-A101 | Two real Renderers share capacity; releasing only scratch still causes output refusal with staged rollback; final owner release permits recovery | New shared-capacity coverage |
| #61-A102 | Real resized scratch refusal request 215 preserves cause, old target pixels, byte/live counts and exact recovery; default Info file and Error stderr, stdout isolated | New default-output observation |

On this same frozen source, 113 render-library cases, 20 desktop composition/error
cases, 64 cases across 12 selected GPU integration suites (including the three
existing full-resolution mask cases), and 69 host-library cases passed:
**266 distinct selected cases**, zero failures or ignores. Workspace/all-targets
FFmpeg check excluding Android and the existing panel style consumer guard also
passed. The 13-case preliminary run overlaps this selection and is not added.
The new desktop case exercises an actual native failure through
CURRENT_COMMAND/respond; it is not a full worker-dispatch test. Local selected
acceptance does not claim full workspace tests or public CI. No panels, styles,
dependencies or assets changed.

Logs and exact-source receipts are
`artifacts/pr-queue/mask-texture-admission-final-{library,desktop,gpu,host,workspace,styles}.*`.
The original failing native control remains in the before-fix log; the acceptance
receipt `.tools/issue-closure-review/mask-texture-admission-acceptance.json`
records source IDs, raw hashes and exclusions. 146 worktrees were inspected;
the old egui worktree's mask changes are the already-main wgpu API migration,
not this budget implementation. That foreign tree was not modified.
Logical native capacity is not physical driver VRAM. Ordinary vector source,
MSAA/resolve and group targets, foreign Presenter integration, complete shared
session/platform pressure, bounded preview/export and repeated lifecycle
acceptance remain pending. #60/#61/#67 remain open; precision and 3D readiness
are not established by this increment.


## Sixteenth increment: native vector source, MSAA and grouped target admission

Accepted product/test source: `b0f239a52b0ba3086b83f8dbff1d378c3fd7c241`.
This enrolls the actual RGBA8 sRGB vector outputs and linear RGBA8 raster targets
in the same allocation allowance as buffers and other admitted native resources.
Four-sample targets reserve their actual sixteen bytes per pixel; the resolve
reserves four. All changed source outputs, ordinary working pairs and missing
group size/depth pairs are staged before replacing any old image, pool or
source-category counter. Texture-stage refusal drops staged reservations and
preserves old identities, cached working targets and already rendered pixels.
The pre-replacement dirty decision is retained so a resized output still draws.
Buffer preparation and subsequent rendering remain separate fallible stages;
this does not claim whole-frame transactional rendering.

The native control `b3f0a240` reproduced 27,744 missing shared bytes for an actual
34x34 ordinary vector source/MSAA/resolve set: booked 1,377,872 versus expected
1,405,616, cargo exit 101. The complete original descriptor-capacity, submitted
pixel and zero-release assertions remain unchanged. Initial implementation
`500253d5` passed that control for both ordinary and grouped vectors.
Six strengthened native cases passed on `300df263` before the final combined run.

Intermediate expanded-case failures are retained honestly: an incomplete fixture
asset table, an attempt to renew existing pending staging ownership, frozen group
geometry that did not actually resize, and a copy destination missing COPY_DST.
The corrected fixtures use the real asset order, completed uploads before the
native texture lifetime scope, rebuilt group geometry, and an independently
admitted copy/readback fixture. Product capture usage, upload ownership semantics,
existing assertions and allocation limits were not weakened for those corrections.

The existing vector buffer suite's original two aggregate-capacity equalities
covered only fixed allocations plus buffers while vector textures were unenrolled.
They now add independently measured descriptor bytes for exactly VectorSources,
VectorRasterScratch and VectorGroupScratch. Comparisons remain exact, native
capacity is independently tested, and final fixed/zero-release assertions remain.
This is an expanded accounting oracle, not an assertion that all old expressions
are unchanged. Existing vertex-phase, old-pixel, recovery and reuse assertions
remain unchanged; those fixtures now keep admitted textures warm and grow real
geometry to reach the same vertex-buffer failure boundary. The before-adjustment
four-case run retained both accounting/site failures and both passing pixel tests.
The first combined desktop run passed20 cases, including the new native-error
case, but retained an earlier texture refusal in the existing vertex fixture:
ellipse bounds rounded to19x18. That fixture now uses axis-aligned Plus geometry
with unchanged18x18 texture bounds and a larger real vertex buffer. The original
vertex-phase assertions are retained, with no product geometry or timeout change.

| Item | Native evidence | Disposition |
| --- | --- | --- |
| #61-A103 | Actual ordinary/grouped descriptors join shared totals alongside buffers; submitted pixels and exact zero release | New native admission coverage |
| #61-A104 | Every ordinary and group depth MSAA/resolve cut point preserves old identities, source bytes, live counts and pixels; below-limit warm reuse and recovery | New texture transaction coverage |
| #61-A105 | Later group refusal rolls back both staged sources and the ordinary working pair across two layers | New cross-layer transaction coverage |
| #61-A106 | Open, finished-abandoned and actually submitted native copies retain textures until their own retirement; later scopes do not renew retired owners | New native command lifetime coverage |
| #61-A107 | Admitted source view retains exactly its native bytes after renderer teardown, then releases to zero | New alias lifetime coverage |
| #61-A108 | Two actual Renderers share capacity; second refusal leaves no reservation; first release enables recovery and actual green pixels | New shared-capacity coverage |
| #61-A109 | Real source refusal request216 preserves typed cause, old pixels, counters and recovery; default Info file, Error stderr and stdout isolation | New default-output observation |

On this same frozen source, 119 render-library cases, 21 desktop composition/error cases, 85 cases across16 selected GPU suites and 69 host-library cases passed: **294 distinct selected cases**, zero failures or ignores. Workspace/all-targets FFmpeg check excluding Android and the existing panel style consumer guard also passed. Preliminary overlapping reruns are not added to this total.

The desktop native-error case exercises an actual native failure through
CURRENT_COMMAND/respond; it does not claim full worker-dispatch integration.
No panels, styles, dependencies, assets, retries, deadlines or project parameters
changed. Local selected acceptance does not claim full workspace tests or public
CI. The previously pending mask batch merged only after all five exact-head public
checks succeeded, using Chinese merge subject/body in PR411 (`b4853443`).

Logs and exact-source receipts are
`artifacts/pr-queue/vector-texture-admission-{before-fix,existing-buffer-scope-control,native-*,final-*}.*`.
The acceptance receipt records raw hashes, selected counts and exclusions.
148 worktrees were inspected; the old egui vector modifications are the already
main wgpu API migration, not shared admission. Foreign Presenter and timeline
work remain untouched. Logical capacity is not physical driver residency.
Presenter admission/ownership, shared session/platform pressure, the complete
bounded preview/export loop and repeated lifecycle acceptance remain pending.
#60/#61/#67 remain open; this increment does not establish precision or3D readiness.
