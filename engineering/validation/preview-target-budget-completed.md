# Preview target budget increment completed

Related work: [#61](https://github.com/MAD-Producer/Kilonova/issues/61), a
prerequisite of [#87](https://github.com/MAD-Producer/Kilonova/issues/87).
Completion here means the preview-target increment, not either whole issue.
The remaining four-prerequisite gates are recorded in
[three-d-prerequisites.md](three-d-prerequisites.md).

## Source and behavior

Initial review used main `159e9810207b61cfe2d59e98c23f16cf8ca36bde`.
Final local acceptance used source
`6544629b6a1626d3ab2223a5903248be87cb5b6a`, including main
`a14b4b518f507e7dc9a2758e14bea0d8bd46e076` and its frozen-reader isolation and
retired-builtin changes. Subsequent completion-document edits do not change the
tested application or Rust assertions.

- Add `previewTargetBudgetBytes` to the existing session policy, defaulting to
  128 MiB for older JSON callers; validate updates atomically
- Check retained front plus candidate capacity before target allocation,
  replacement or reuse, including when an existing allowance is lowered
- Charge actual target format capacity, including RGBA16Float, and retain the
  existing per-target pixel and device-dimension limits
- Start the invalid host placeholder at one pixel; use the existing four-level
  degradation, cooldown and recovery state machine for requested frames
- Preserve front pixels after refusal, avoid a third retained application target,
  and keep formal PNG capture at full resolution
- Return policy version 3 consistently from query, update and preview snapshot

## Local acceptance

All commands used the portable MSVC environment and build/temp/output directories
on E. No source assertions, test ignores or timeouts were relaxed.

| Command/selection | Result |
| --- | --- |
| `cargo test -p motion-render --test preview_target_budget --test preview_memory --test memory_policy --test texture_memory --test resource_rejections --test device_scratch_budget --test vector_budget_alignment --test video_lifetime --locked -- --test-threads=1` | 37 passed, 0 failed, 0 ignored |
| `cargo test -p motion-host --lib --test memory_policy --test preview_memory --locked -- --test-threads=1` | 73 passed, 0 failed, 0 ignored |
| `cargo test -p motion-desktop-media --test preview_budget_export --locked -- --test-threads=1` | 1 passed, 0 failed, 0 ignored |
| `cargo check --workspace --exclude motion-android --locked` | Passed; existing desktop warnings remain |

Six new tests cover real allocation refusals, unchanged retained pixels, candidate
replacement/reuse, format-aware capacity, bounded exhaustion and recovery,
legacy JSON compatibility, invalid updates, and actual frozen PNG export after
editing and closing the live session. The frozen export test uses the minimum
eight-byte preview allowance and checks every output pixel at the full size.
Existing host policy tests also verify query/update/snapshot version consistency
and inheritance by the editor renderer.

Source-bound local logs are `artifacts/pr-queue/preview-target-budget-current-main-*.log`;
the orchestration record retains exit codes and SHA-256 hashes. Earlier logs are
preserved separately, including an initial test compile mistake and an earlier
host invocation without a terminal result for its final group. Those attempts
are not counted as full successful acceptance runs. Final acceptance above uses
the current source and complete results.

## Limits and pending work

This does not implement a combined frame reservation allocator or a physical
GPU memory cap. Other texture classes, buffers, swapchain, decoder allocations,
other renderers and retired submissions require separate accounting. Format-aware
target capacity does not enable end-to-end float compositing or float readback.
The packed plan/GLES ABI is unchanged. No Android device acceptance or FFmpeg
movie export was performed for this increment; the existing FFmpeg-only fixture
is updated for the new policy field, and the new PNG export runs without FFmpeg.

No panel, layout, style, node or camera production code is changed. Repository
provenance and panel-style guards pass. Native CI results for the eventual PR
must be checked independently; these local results are not a claim that all
workspace or desktop tests have already run on every platform. #61, #72, #74,
#78 and #87 remain open until their full acceptance requirements are met.
