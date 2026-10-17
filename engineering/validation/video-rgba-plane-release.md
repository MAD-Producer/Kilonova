# Video RGBA handoff and resource lifetime audit

Branch `fix/video-rgba-plane-release`, stacked on PR #314 at `01e0da5`.
Related to #68 and source-memory accounting in #61. This increment fixes a
reproduced handoff defect; it does not close either aggregate issue.

## Reproduction and functional fix

Before the production change, four new GPU regressions failed and one existing
standalone-input regression passed. Evidence:
`artifacts/workspace-stall/video-rgba-planes-before.log`.

- A completed 16x16 YUV upload retained 384 plane bytes after RGBA replaced it.
  With two live instances, the converted instance kept its obsolete planes in
  addition to the other instance's still-needed planes (768 instead of 384).
- The same-frame fast path reused valid converted RGBA pixels but also retained
  the no-longer-needed planes.
- Growing that RGBA output to 32x16 incorrectly charged 2436 bytes against a
  2052-byte source budget, even though its final owned set is exactly 2052 bytes
  (four-byte fallback plus 2048-byte output).
- A successful RGBA upload after cancelling a YUV encoder still retained its
  conversion inputs.

Successful RGBA handoff now removes only that object's YUV inputs. Source-budget
and idle-image eviction calculations credit only those inputs scheduled for
retirement. Other objects' planes remain charged. Old completion/cancellation
state remains intact until a replacement succeeds, so a failed replacement
cannot turn an unsubmitted or cancelled conversion into a valid cache hit.
Same-stamp completed outputs are reused without another upload.

This is logical owned-resource accounting, consistent with existing replacement
checks. It is not a physical VRAM/instantaneous peak bound: queued GPU work and
backend retention can outlive application ownership. No texture is explicitly
destroyed while queued work may use it; existing wgpu ownership semantics remain.
No idle video cache, retry, cancellation-policy change, or worker restart is added.

## Error-handling audit

| ID | Classification | Evidence and remaining limits |
| --- | --- | --- |
| VR-01 | Functional fix | Five GPU regression cases cover completed/same-PTS handoff, repeated YUV/RGBA switching, per-instance ledger ownership, unaffected peer pixels, budget credit, and cleanup. The two instances use separate 16x16 rectangles; exact blue and CPU-converted YUV pixel assertions remain strict. A two-instance budget test rejects 3459 bytes and accepts 3460 bytes while retaining the peer's planes and cache hit. |
| VR-02 | Failure boundary retained | A pending encoder still rejects RGBA replacement. After cancellation, a budget-rejected replacement preserves old memory and cancellation state; retrying the same YUV PTS still produces an upload token. Only a successful replacement releases its planes. |
| VR-03 | New bounded error context | Resource refusals retain the original `RenderError::Resource` and `ResourceRejection`, with object, source, PTS, phase, and the typed required/budget values added to the message. The original cause remains. Context contains bounded IDs/numbers/enums, no pixel or user-text payload. |
| VR-04 | Existing observations verified | A desktop subprocess causes an actual low-budget GPU upload refusal, checks the typed byte counts and unchanged ownership, then uses the production reply boundary with controlled origin context. At default level it checks the original reply, file Error, Error stderr, context, and stdout isolation; a subsequent allowed replacement succeeds. This is a real renderer failure with controlled request context, not a full playback-session reproduction. |
| VR-05 | Normal paths retained | Valid cache hits, inactive clips, and aborted encoders are not logged as faults. Existing active-set deletion, queued-work ownership, and best-effort reply semantics remain. No new counters or hot-path logger calls are introduced; the existing outer reply logger runs after business locks are released. |
| VR-06 | Existing coverage | Executed GPU suites cover 60-clip forward/reverse traversal, hidden matte demand, standalone switches, distinct PTS instances, nested/unsubmitted conversion, source-budget refusal, project/source replacement, and instrumented queue completion. See commands/results below. |
| VR-07 | Pending, not completed by audit | Cross-instance same-source/PTS GPU-output sharing, physical/external-surface peak accounting, and a combined native frozen-export/cancellation/reopen lifecycle stress test remain outside this increment. #68/#61 remain open; a bounded per-instance allocation is not presented as an unbounded leak. |

No log call is added inside renderer/business-state locks. Only failed resource
returns are enriched; successful uploads add no file IO or per-frame warnings.
Panic hooks, non-Present start publication, Present timing policy, thresholds,
and reply semantics are unchanged.

## Entry-point audit

| Path | Current ownership/reclamation boundary | Evidence scope |
| --- | --- | --- |
| Interactive preview | `Session::present` calls renderer `retain_video_instances` before video preparation. | Static entry audit; shared upload/reclamation code exercised by GPU regressions. |
| Still capture | Host `ops::export::capture` retains before and after exact video preparation. | Static entry audit; actual renderer capture/pixels exercised in tests. |
| Frozen movie export | `FrozenMovie::export` retains video instances for every sampled scene before exact preparation. | Static audit only for this increment; combined native cancellation stress remains pending. |
| Nested scenes | `retain_video_instances` recursively collects video/composition instances; scene alias IDs separate repeated sources. | Existing video/nested and memory-ledger GPU suites. |
| Standalone authoring input | `PreviewInputTexture` owns its conversion resources independently. | Existing 60 YUV/RGBA switches plus transparent reset test. |
| Project/asset replacement | Renderer asset replacement clears/replaces owned video state; `clear_assets` releases planes and outputs. | Existing resource-identity/project-switch GPU regression and new handoff cleanup assertions. |

## Verification

```text
cargo test --locked --offline -j1 -p motion-render \
  --test video_lifetime --test video_yuv --test video_instances \
  --test texture_memory --test memory_policy --test submission_memory \
  -- --test-threads=1

cargo test --locked --offline -j1 -p motion-desktop --bin motion-studio \
  engine::failure::composition_tests::video_handoff_budget_failure_reaches_reply_file_and_stderr_at_default_level \
  -- --exact

cargo test --locked --offline -j1 -p motion-render --test video_lifetime \
  rgba_replacement_cannot_borrow_another_live_instances_plane_budget -- --exact
```

Windows portable MSVC, owned isolated target
`artifacts/workspace-stall/desktop-frame-layout-target`, debug information and
incremental compilation disabled. The renderer suites passed 28 tests and the
desktop failure-channel test passed one. The subsequently added peer-budget
regression passed separately: 30 distinct tests passed, zero failed/ignored,
including six new tests. Repeated runs of the same test are not counted again.
Logs: `artifacts/workspace-stall/video-rgba-planes-tests.log` and
`artifacts/workspace-stall/video-rgba-planes-desktop.log`.
The additional test is recorded in `video-rgba-planes-peer-budget.log`.

Actual default-level output was also retained by running the child test directly:
`artifacts/workspace-stall/video-rgba-planes-default-output/motion-studio.log`,
`video-rgba-planes-default-stderr.log`, and `video-rgba-planes-default-stdout.log`.
File and stderr both contain `error #200 present 1ms`, `object=1 source=1 pts_us=1`,
`phase=rgba_upload`, `required_bytes: 2052`, `budget_bytes: 4`, and the original
budget cause. Stdout contains only test-runner output. Existing environment
Vulkan implicit-layer configuration errors are separately logged; this increment
does not fix them or claim an error-free environment.

No visible UI, dependency, third-party asset, package protocol, or project format
changes. Existing assertions are unchanged; the new pixel fixture explicitly
avoids two rectangles occluding each other. New test code uses independently
generated solid RGBA/YUV buffers. Existing vendored/dead-code warnings remain.
