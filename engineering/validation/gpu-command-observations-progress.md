# GPU command observations: implementation and remaining acceptance

This is a progress record for issue #60, not a completion claim. Android validation is excluded by the user's confirmed scope. No optimization gain, exclusive GPU busy time, complete pass coverage or new performance baseline is claimed.

## Delivered predecessor

PR #466 was merged as `81aca69996a629643faebc71345c60afe054a6e1` with a Chinese squash title and body. Its actual final head `d8654590bbf658a0de7ffb95960a81e57200bbd7` passed the dependency audit, Rust/GPU and three maintained PR desktop checks; two release-only jobs were skipped. That PR contains the pressure/readback completion documents and immutable source-attributed archives. Its measurements are not measurements of this later GPU implementation.

## Actual local evidence

| Source | Actual acceptance | Limits |
| --- | --- | --- |
| `4e0bdddc84206577fbae49aa1a79c59378c869f8` | Standard timestamp boundary feasibility, 1 passed; original query words and 64 real pixels | Local RTX 4070 Laptop/Vulkan only, optional encoder-timestamp feature disabled |
| `9aba57194ee2fee4f6b049944dac2f7c32ba846e` | Full render/host suites: 683 passed, 0 failed, 1 existing external-fixture ignore; ffmpeg desktop check passed | Earlier primitive source, before automatic host attachment |
| `da2258ec2c7cfe1ec31e7e838d3b8de1a9fde729` | 12 focused tests passed: 6 native cases and 6 metadata/log-scope cases; host, default desktop and ffmpeg desktop checks passed | Full regression, real file/stderr failure integration and whole-path baselines for this source remain pending |

The last source is based on merged main `81aca699`, with this branch's GPU commits reapplied. The earlier source `44de827f` failed 5/1/0 because cancellation cleanup called `unmap` after an actual cancelled mapping. Source `9aba5719` fixed that native cleanup defect without weakening the assertions. The original failure log and its disposition remain retained.

Local receipts are under `E:/Dev/aem/artifacts/pr-queue`: `issue60-gpu-command-marker-v1-*`, `issue60-gpu-command-primitive-v1-*`, `issue60-gpu-command-primitive-v2-*`, `issue60-gpu-command-broad-v1-*` and `issue60-gpu-command-integration-v1-*`. These new GPU records have not yet been published as an immutable archive or accepted by PR CI.

## Implemented observations

| Item | Behavior and evidence |
| --- | --- |
| G1: non-returning encoding | Publish Encoding before native encoder creation; preserve request/allocation-frame context. Controlled blocking verifies live metadata; native tests verify Encoding and Encoded before completion |
| G2: bounded state | Maximum 128 active commands, 256 recent records and 64 contexts; nonwrapping epoch/command IDs, omission counts, saturating cumulative counters, bounded original causes and deferred log events |
| G3: real callbacks | Native tests read original valid words, render/check pixels, cancel actual mappings, recover slots, inject invalid diagnostic words through a real GPU copy, and preserve submission-unwind cleanup ordering |
| G4: ownership and retirement | Budget/queue rejection remains before a submission attempt; unconfirmed attempts quarantine slots and retain timing leases through a real queue completion. A read-only observer retains metadata instead of native buffers |
| G5: automatic explicit attachment | `Renderer::start_command_profiling` opts in; ordinary rendering allocates no command timer and inserts no markers. Commands join only currently CPU-active allowance windows. Callback epochs survive restart and cannot become a later frame |
| G6: raw exposure submission | Observe the existing ExposureMedia raw route while preserving `submit_tracked` retirement, instead of converting it into an allocated-encoder reservation route. Native wrapper tests verify no new allocation-encoder scope and exactly one original frame submission |
| G7: deferred escalation | Callbacks retain events only; first failed sample is Debug, occurrence 60 is Error once. Successful samples and explicit callback cancellation reset the streak. Unmapped work, capacity omissions and discarded encoders cannot falsely prove recovery. Scope tests check Info filtering, suppression, bounds and flush after business/observation guards, including unwind |

Native diagnostic corruption is an injected failure, not evidence of a hardware/driver fault. The six real cases cover independent intervals, cancellation/reuse, submission unwind, automatic active-frame association/restart, invalid-word escalation/recovery and the raw wrapper. They do not demonstrate every production scene route.

The ordinary root timer remains separate. Command intervals include marker overhead, root-query work inside the encoder, and possible backend scheduling gaps. Queue-generated writes before the first marker are outside them. Overlapping root and command intervals must not be added or described as exclusive GPU busy time.

## Pending work

- Carry a stable exposure request identity across pending polls and a distinct poll identity through supersession; current exposure-specific context fields remain unknown
- Attach independent PreviewInput owners explicitly without merging allocation owners or putting a device-specific query pool on a shared multi-device budget
- Collect a bounded full recording trace with exact command/epoch/frame links; the 256-record diagnostic ring alone cannot supply every earlier frame of a long report
- Verify actual default-level file output, Error stderr and stdout isolation for the new deferred failure path through a real desktop session
- Execute complete appropriate regressions for the final integrated source and actual native nested/vector/mask/effect/exposure/video/overlay paths, preserving exact source, quality, resolution and driver attribution
- Publish the new raw evidence and completion dispositions only after those gates pass; retain the existing application's allowance-pressure baseline without presenting it as physical/OS pressure acceptance

Issue #60 remains open. Neither compilation nor the focused test count closes these remaining requirements.
