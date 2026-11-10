# Native worker foreground yield controls (#82)

Accepted worker-only source: `165753e03fd4f33c206a67bdfeb5faba92928759`. Android is excluded. The new
yield control is not yet connected through Coordinator/Engine dispatch or actual
playback demand. General prerender capability stays false. Existing idle Engine
checkpoints remain separately source-bound; this is not complete #82 acceptance.

A single-use YieldControl owns its original cancellation control and completion
flag. Foreground yield cancels running work; after native completion it preserves
the bounded result for foreground consumption. Explicit cancellation remains
unconditional, including after completion publication. Reusing a claimed yield
token cannot admit work in the same or another owner. The worker publishes the
flag only after native work/retirement ends and before sending its bounded reply.
Cancellation racing with this flag may conservatively discard a completed frame;
it cannot authorize an invalidated frame. Tokens do not retain image bytes or
escape the actual CPU completion permit.

Actual controls cover yield before enqueue, a blocked native draw retaining its
CPU slot until acknowledgment, exact completed red pixels surviving foreground
yield, token reuse refusal, and explicit invalidation discarding that result.
Existing strong-cancellation, panic retirement, logging and ownership assertions
were retained and passed.

Worker suite: 10 passed, zero failures/exclusions. These tests overlap the full
media regression: 95 passed, zero failures, two explicit manual exclusions.
The excluded keyframe output and all-four-precision producer controls are listed
individually in the receipt and are not new acceptance. Desktop check with FFmpeg
and ai-depth, and provenance validation passed. Full current Engine/Host/Render
regression, runtime yield integration, actual active-playback warm hits, external
source refresh, broader profiles, pressure controls, paired P95 and cross-platform
CI remain pending. No existing assertion was weakened. No disk cache is provided.

Archive: `data/native-prerender-yield-v1.zip`.
SHA-256: `cfd3a5c400593243a442105e4d8e7f5dd9c847109a90476070d86c3639ffaf8f`. All 11 members were read back byte for byte.
