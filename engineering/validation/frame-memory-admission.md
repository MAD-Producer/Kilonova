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
