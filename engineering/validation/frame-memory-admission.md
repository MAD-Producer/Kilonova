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

The first full workspace/all-targets FFmpeg compile passed. Final source-bound
GPU and host regression evidence will be appended after execution; the earlier
three unit and six GPU passes describe an intermediate source only.
