# Issue closure review: #130, #107, #66 and #133

Review date: 2026-10-19. Baseline: main
`452f659d6c92ebfe60554b21bd26f45e3829f5da`.

## Dispositions

| Issue | Disposition | Evidence and remaining limits |
| --- | --- | --- |
| [#107](https://github.com/MAD-Producer/Kilonova/issues/107) | Closed as a completed umbrella | Administrative consolidation, not completion of every unchecked interaction; retain the original checklist and independent child issues |
| [#66](https://github.com/MAD-Producer/Kilonova/issues/66) | Closed as not planned | Android maintenance and automated delivery are disabled; the full compression/packaging/negotiation proposal is outside the current product schedule, not implemented |
| [#130](https://github.com/MAD-Producer/Kilonova/issues/130) | Keep open | Existing timeout/backpressure/shutdown work does not complete the global session lock, cache-lock partitioning or all background-worker lifetime requirements |
| [#133](https://github.com/MAD-Producer/Kilonova/issues/133) | Prepare the remaining compositor guard for review | The old assertion has an internal invariant; replace it with an indexed error and verify failure and normal geometry paths; closure waits for the change to merge |

Recorded Chinese explanations:

- [#107 umbrella disposition](https://github.com/MAD-Producer/Kilonova/issues/107#issuecomment-6264610627)
- [#66 scope disposition](https://github.com/MAD-Producer/Kilonova/issues/66#issuecomment-6264610061)
- [#130 remaining paths](https://github.com/MAD-Producer/Kilonova/issues/130#issuecomment-6264622775)

#107 retains references to #253, #216, #217, #218, #227, #228, #204, #205 and
#277. #253 covers the new-composition menu entry, not every file-lifecycle demand.
Keyframe clipboard and field-commit requirements in the original body must not be
counted as delivered merely because the umbrella is closed. No checkbox was
changed to imply missing work was implemented.

#66 is not technically Android-only: its original proposal also includes desktop
WGPU texture resources. Closure is a product-scope decision. Reopen and re-scope
it if Android maintenance or desktop compressed-texture work is scheduled.
README line 5 and `.github/workflows/ci.yml` line 71 record the Android maintenance
and workspace-test exclusions.

## #130: corrections to the proposed closure

- `Engine::call_with_timeout` bounds the caller's wait to 60 seconds. It does not
  interrupt a native/GPU call or prove responsiveness throughout a ten-second GPU
  operation. `float_window` releases the factory guard before the blocking call;
  command dispatch uses a bounded `sync_channel` and `try_send`.
- Shared `Arc<Engine>` ownership is a design choice. `main` explicitly requests
  shutdown after the event loop returns; `Engine::Drop` requests it again and
  joins the engine worker. This is independent of an MCP reader retaining an Arc.
- `Session::new` is constructed outside the registry lock in `open`, but that
  does not move all GPU creation outside the lock. `with_session` still calls
  `operation(session)` while holding `sessions()`. `ops/export.rs` creates a
  headless renderer inside that closure; plugin `editor_preview` can also create
  one through the locked session operation.
- `image_resources.rs` still defines a process-wide `CACHE_LOCK`; proxy reads and
  writes perform filesystem IO within the guard. Recovery from poisoning and
  releasing the lock before logging do not partition this lock by project.
- `MovieExport::Drop` and `DecodeTask::Drop` request cancellation. `VideoFrames`
  reaps only completed retired workers. Joining the main engine worker is not
  proof that all background native calls have returned or all threads are gone.
- The cited `logging.rs` `writer.lock().unwrap()` is in a test fixture, not a
  production sink. This review does not claim a new exhaustive production-lock
  audit or fault-injection acceptance for #130.

No lock redesign, forced worker restart, cross-thread GPU transfer, timeout change
or background-thread join was added in this review.

## #133: plane invariant and defensive change

The seven sections' previous changes are on main through merged PRs #292, #294,
#295, #296 and #297. The earlier issue comments corrected an erroneous PR-number
sequence; #293 is not counted as one of those fixes.

For the remaining compositor assertion:

1. `prepare_with_bounds_and_overlays` rejects more than `MAX_LAYERS` layers.
2. Each spatial layer calls `quad`, which overwrites `planes[layer]` before use.
3. Degenerate planes are skipped before either root-node creation or insertion.
4. `insert` only creates nodes from those polygons or `clip` fragments; `clip`
   preserves the original layer index. No intervening path overwrites that layer's
   plane while its polygons are inserted.

Therefore this is an internal invariant on current public construction paths,
not a newly reproduced project-driven panic. The defensive change uses
`get(...).copied().flatten()` and returns `motion_model::Error::Invalid`, including
`layer_index`, before changing the node list. Both missing and out-of-range state
fail without panic; successful ordering and error propagation are unchanged.

Added CPU regressions directly inject missing/out-of-range plane state and verify
unchanged output containers. A public prepare-path regression exercises two
intersecting planes, skipped degenerate geometry and repeated preparation after a
formerly valid plane becomes degenerate. The injection is not presented as a
reachable user-project reproducer.

## Validation

Targeted compositor regressions: 2 passed. Final local validation:

```text
cargo test -p motion-render --lib --test plane_intersections --test orthographic_planes -- --test-threads=1
```

- Library tests: 27 passed, 0 failed, 0 ignored
- Orthographic plane GPU tests: 2 passed, 0 failed, 0 ignored
- Intersecting plane GPU test: 1 passed, 0 failed, 0 ignored

The original GPU tests retain their pixel assertions for crossing opaque and
translucent planes, stack reversal, transparent cutouts, overlays, orthographic
views and mirrored layers. Log:
`E:/Dev/aem/artifacts/issue-133-render-validation.log`. The initial focused run is
`E:/Dev/aem/artifacts/issue-133-compositor-tests.log`. These are focused renderer
checks, not a claim that the complete workspace/GPU suite was rerun locally.

The change adds no dependency, asset, UI layout or interaction. No panel file is
modified. Historical Android decoding is outside the maintained desktop scope;
this review does not certify the paused Android backend or every unrelated
assertion across the repository.
