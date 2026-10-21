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
