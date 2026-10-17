# Composition frame mapping validation

Parent: PR #313 (`fix/composition-input-boundaries`), commit
`382378d`. Branch: `fix/composition-frame-mapping`. This increment completes
the remaining local frame-rate guard in #132; the aggregate issue remains open.
No visible UI, new dependency, asset, retry, worker restart, or file-format
change is included.

## Functional and API boundaries

`CompositionClip::source_frame` now returns `Result<f64>`. A zero parent rate
previously produced NaN/infinity, and a zero child rate collapsed every source
time onto the offset. Both return structured `invalid_frame_rate` errors.
Non-finite input or arithmetic overflow returns `invalid_time`. The valid
mapping formula is unchanged, including negative offsets and fractional times.

Scene sampling propagates these failures before range culling. Image lookahead
now returns `Result<Vec<u64>>`, propagates nested mapping failures, and rejects
zero root fps before dividing by it. The host returns that failure through its
existing `Result` rather than supplying an empty successful prefetch plan.
These are Rust API signature changes; all repository callers are updated.

Render-side errors retain the original model code, target, message, and rate/time
details, adding the parent composition, reference-layer ID, and phase
(`scene_sample` or `image_prefetch`). Error context is allocated only on failure.
The root-prefetch error has no reference-layer ID because it precedes traversal.

The negative/non-finite prefetch query convention remains a normal empty
lookahead. Negative mapped frames outside a child clip remain normal culling.
Neither is promoted to a logged fault. Valid imports and ordinary editing already
reject zero fps; this is defense for direct public API calls, not a claim that a
validated document can reach these zero-fps paths.

## Audit against AGENTS.md

| ID | Classification | Evidence and limits |
| --- | --- | --- |
| CM-01 | Functional/protocol boundary | Three model tests cover zero rates, non-finite times and overflow, and 1,920 exact comparisons of valid mixed-rate mappings with offset and fractional/negative time. Errors remain typed and retain original causes. |
| CM-02 | New error context and propagation | Two render tests exercise real scene sampling/prefetch failures with parent/reference/phase details, successful recovery after correcting rates, and retained normal misses. No fallback swallows an error. |
| CM-03 | Existing successful behavior | Existing image-resource/cache and GPU composition suites retain their expected IDs, pixels, clip timing, and cache behavior. Existing prefetch tests only unwrap the newly fallible API; expected values and assertions are unchanged. |
| CM-04 | Existing observations verified with actual failures | A new desktop subprocess test causes actual sampling, prefetch, and NaN-mapping failures, then passes each to the production `respond` boundary with controlled request context. At default level it asserts the original reply and structured code, file Error records, Error stderr, and stdout isolation. It uses public APIs with deliberately invalid inputs; it does not inject an invalid engine session or claim to reproduce this via ordinary project loading. |
| CM-05 | Existing worker/sink coverage | The full real-worker capacity test from #313 also runs after the API changes. It separately covers Info start order, rollback, cleared in-flight state, continued editing, and actual default-level file/stderr/stdout routing. No new logger, thresholds, counters, panic hook, or blocking handler is introduced here. |
| CM-06 | Pending issue work | This completes the local `source_frame`/`audio_voices` rate-guard item together with #313. It does not establish all #132 acceptance criteria: graph-candidate allegations, storage directory durability, and external image-path contracts still need their own dispositions and evidence. |

No business-state or observation lock encloses a new logger call. Normal Present
calls remain quiet; only actual failures reach the existing reply Error logger.
No new Info start is added to Present. Protocol best-effort delivery and failure
return values are preserved.

## Verification

```text
cargo test --locked --offline -j1 \
  -p motion-model --test composition_frame_mapping --test composition_boundaries \
  -p motion-render --test composition_frame_boundaries --test image_resources \
  --test image_preview_cache --test compositions -- --test-threads=1

cargo test --locked --offline -j1 -p motion-desktop --bin motion-studio \
  engine::failure::composition_tests:: -- --test-threads=1
```

Run with the portable Windows MSVC environment and the owned isolated target
`artifacts/workspace-stall/desktop-frame-layout-target`, with dev/test debug info
and incremental compilation disabled. The final run passed **26 model/render
tests and 2 desktop tests (28 total)**, with one existing optional image probe
ignored. Six tests are new (three model, two render, one desktop). New direct
API fixtures pass normal project validation before deliberately invalidating
their frame rates. The actual GPU suites pass without changing pixel assertions.
Logs:

- `artifacts/workspace-stall/composition-frame-mapping-tests.log`
- `artifacts/workspace-stall/composition-frame-mapping-desktop.log`

A direct invocation of the same public-API/reply subprocess case also retains
the actual outputs as local evidence:

- `artifacts/workspace-stall/composition-frame-mapping-default-output/motion-studio.log`
- `artifacts/workspace-stall/composition-frame-mapping-default-stderr.log`
- `artifacts/workspace-stall/composition-frame-mapping-default-stdout.log`

New test files pass rustfmt checks; the staged diff passes whitespace checks
and the source/provenance guard (1369 tracked paths).

The existing optional external-image probe remains ignored when
`MOTION_IMAGE_SOURCE` is not supplied. Existing vendored/dead-code warnings are
outside this change. No existing assertion was weakened to meet acceptance.
