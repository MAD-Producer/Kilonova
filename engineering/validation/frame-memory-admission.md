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

## Fourth increment: exposure accumulation admission (pending acceptance)

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
