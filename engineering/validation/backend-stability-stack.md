# Backend stability increment on PR #307

Base: PR #307 (`fix/silent-failure-paths`), commit
`97a3797f25ab6327c0c9e86b0fe2bb6c18f16b3c`. Branch:
`fix/backend-stability-stack`. Original work is preserved on
`fix/backend-stability` at `141cc5c`.

This increment reuses #307's cache publication/rollback implementation and
worker observations. It adds no dependencies, assets, panel presentation changes,
or Android-specific implementation. Subsequent PRs should use #307's head branch
as their base while it is open. After its squash merge, rebase only the child
commits onto the merged `main`, using the pinned parent head above as the old
base; then retarget the PR and check the incremental diff and CI again. Changing
the target branch alone can expose the parent's commits again.

## Scope and issue boundaries

| Issue | Increment | Remaining scope; do not close the whole issue |
| --- | --- | --- |
| #131 | Audio/video cache rebuild integration tests preserve the old cache when the second destination is a directory, report failure through task status, and permit retry. The publication implementation comes from #307. | Other decoder/encoder boundary and lifecycle items require separate verification. |
| #132 | Read `disabled.json` with a 512 KiB bound. Malformed, oversized, or unreadable settings preserve the file and enter registry diagnostics rather than blocking registry creation. Valid settings still disable plugins. | This recovery uses the default empty disabled set, so previously disabled plugins may be enabled. The diagnostic explicitly states this. Other model/storage/key collision items remain outside this increment. |
| #130 | A 128-command bounded engine queue rejects new work immediately on saturation. Accepted edits/save drain in order. Node preview expectations coalesce separately. Explicit shutdown works while another MCP `Arc` remains alive. | Existing synchronous UI calls and uninterruptible native GPU/codec calls remain. This is no claim that every shutdown now has a time bound. |
| #61 / #68 | Retiring decoder workers retain their slots until their threads finish. Active plus retiring workers per `VideoFrames` reader cannot exceed four. Scene changes and source replacement both use retirement. Pending replies name `decoder_retirement`. | The cap is per reader, not process-wide. Dropping an entire reader can detach a slow native call after sending its stop signal. No platform VRAM queries or global session budget are added. |
| #60 | Existing bounded frame recordings gain separate resource samples: logical texture capacity, presentation target capacity, CPU video cache/proxy bytes, video preparation/upload timings, cumulative upload/pipeline counters, quality level and refusal count. | Successful frame boundaries are samples, not transient peaks or physical VRAM residency. No new device benchmark, display-deadline trace, conversion/readback counters, or performance improvement claim is included. |

In the resource samples, `pipelineBuildsTotal` counts renderer effect-pipeline
cache builds, not every GPU pipeline created by the application. Upload totals
are the renderer's cumulative image/video counters, not a measurement of all
driver transfer traffic.

## Error propagation

- Queue saturation returns an error before accepting the command; accepted work
  is never silently replaced. Only the explicitly obsolete node expectation
  value is coalesced.
- A poisoned node expectation lock follows the engine's failure/reply boundary;
  subsequent calls report the recorded failure rather than waiting indefinitely.
- Decoder stop failures are retained in metrics and returned by the next request.
  Finished worker join failures preserve their panic payload in the returned
  error and `retirementError` metric, as well as the log.
- Registry recovery enters `Registry::diagnostics`, already returned by the
  plugin listing operation's `errors` field. It does not overwrite the damaged
  settings file on load.
- Frame resource accounting errors propagate through the host operation. Native
  event-loop failure still requests engine shutdown and preserves its original
  error; shutdown-request errors are logged and returned.

## Validation

Validation is against the stacked worktree, not the earlier main-based WIP.
Commands use the portable MSVC environment, `CARGO_BUILD_JOBS=1` and
`CARGO_TARGET_DIR=E:/Dev/aem/target`. Existing vendor or desktop warnings do not
count as a clean-warning build. No distributable executable is produced by these
test commands.

| Check | Result | Log under `E:/Dev/aem/artifacts/workspace-stall/` |
| --- | --- | --- |
| Targeted media/effects/host/render integration tests below | 37 passed, 2 existing large-media tests ignored | `backend-stability-stack-core.log` |
| `cargo test --locked --offline -j1 -p motion-desktop engine:: -- --test-threads=1` | Final source: 18 passed, 343 filtered | `backend-stability-stack-engine-final.log` |
| `cargo test --locked --offline -j1 -p motion-host --lib video_frames:: -- --test-threads=1` | 9 passed, 17 filtered | `backend-stability-stack-workers-final.log` |

Integration command:

```text
cargo test --locked --offline -j1 -p motion-render --test frame_measurements -p motion-host --test video_retirement --test memory_policy --test preview_memory --test preview_inputs -p motion-effects --test registry_recovery -p motion-media --test audio_backend --test video_transactions
```

The ignored audio test generates 128 MiB sources and a project over 256 MiB and
is marked for explicit release-mode execution. The ignored video test requires
an external >512 MiB MP4/AAC source. Neither large-media case was verified here.
`git diff --check` and the provenance guard passed. The library-test fixture was
updated for the newly owned worker handle before the final rerun.

Across these final checks, 64 distinct tests passed and 2 existing large-media
tests were ignored. No final runtime test failed. This is a targeted validation,
not a claim that the entire workspace test suite ran. The stack is prepared in
local commits; no push, PR creation, merge or issue state change was performed
during this preparation.

Targeted coverage includes full queue backpressure, 10,000 coalesced expectation
updates, accepted edit/save preservation, undo, idle and full shutdown, project
lock release with an extra `Arc`, poisoned expectation replies, blocked native
decoders, source replacement, 60 clear/recreate cycles, retirement panic payloads,
damaged registry settings, blocked cache destinations and bounded resource
recordings with missing samples.
