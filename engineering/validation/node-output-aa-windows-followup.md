# Node output coverage: Windows CI follow-up

## Observed failure

PR #337 at `c09012bb966a0743a91093731010f1bc08d20146` failed the
Windows Server 2022 desktop tests in both the push run `39197438910`
and pull-request run `39197501372`. Both report the same two failures:

- `attached_hidden_surface_and_factory_release_after_worker_panic`
- `pending_video_presents_chrome_without_acknowledging_an_unready_composition`

The latter reports a lost DX12 device on Microsoft Basic Render Driver
`10.0.20348.5386`. The former reaches an invalid mapped buffer while
installing a node preview. Neither failure is a changed test assertion.
The preceding main commit `f32c64d` passed its Windows desktop job
`117853733104` in run `39187775033`.

The pending-video test passed locally on DX12 both with the normal adapter
selection and with a temporary `force_fallback_adapter: true` diagnostic
override. That override was restored immediately after the run and is not
part of this patch. Passing a newer local driver does not establish that
the older CI driver is correct, nor establish the exact cause of device loss.

## Candidate compatibility change

Rewrite the half-plane integral without derivative-dependent early returns.
Guard all denominators, including operands discarded by `select`, and use
ratios for the tail instead of multiplying two derivative magnitudes in
the denominator. This retains the square-pixel integral, axis-aligned limit,
constant-derivative limit, and original thresholds. It does not disable
antialiasing, change adapter selection, add retries, or skip tests.

Extend the existing independent polygon/pixel-area GPU oracle to rotations
of 0, +0.000001, -0.000001, 3.7, 45 and 89.99999 degrees. Keep the existing
partial-pixel and error assertions unchanged. The driver compatibility
hypothesis requires a fresh Windows Server 2022 CI result before it can be
reported as a confirmed repair.

## Evidence boundaries

The full `output_antialiasing` integration target passed all five tests
locally, including the six-angle polygon-area oracle. Summed alpha error
against the pixel-area oracle ranges from 0.486 to 0.765 pixel equivalents,
versus 92.6 to 123.9 for the binary-edge reference. Original assertions and
thresholds are unchanged. Source guard and provenance checks also pass.
The local renderer suite passed 205 tests (one existing optional-image test
ignored), and the desktop suite passed 566 tests (one existing native-audio
test ignored).
Original failure logs and local diagnostic runs are retained under
`artifacts/node-compact-examples/ci-surface-regression/`.

The four native example runs and executable described in
`node-compact-examples-completed.md` remain evidence for the original
`4abb801` implementation. They are not rebuilt screenshots or binary
acceptance for this follow-up. The broader node-editor audit remains open.

## Subsequent Windows result

At `85560a5`, pull-request job `117888987248` passed all 574 desktop tests
(one existing test ignored), including both original failures. It then failed
the host integration test
`proxy_cache_survives_cancelled_upload_and_reclaim_without_retaining_the_decoded_frame`
at its unchanged two-byte-per-channel pixel comparison. This later failure
was not reached in the previous failing runs. The merge run includes newer
main tests, which accounts for its larger desktop test count.

The proxy test passes locally on the normal backend, hardware DX12, and
DX12 with a temporary fallback-adapter override. The override was restored.
Add adapter information, expected RGBA and up to eight mismatching pixels
to the assertion message so another Windows result can distinguish color
conversion from plane coverage. Keep the comparison and tolerance unchanged.
The complete local `preview_memory` target passes all three tests. The
Windows pixel discrepancy and overall PR CI result remain unresolved.
