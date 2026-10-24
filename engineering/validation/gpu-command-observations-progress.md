# GPU command observations: historical implementation notes

The current frozen ff52 correctness and Release increments are accepted locally. See [accepted correctness](gpu-command-observations-completed.md), [Release measurement](gpu-command-release-completed.md) and [original requirement audit](issue60-desktop-scope-completed.md). Final-head PR CI remains required before delivery.

The notes below retain the original per-source progress history. Pending statements describe those earlier stages, not the later ff52 acceptance.

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
| `63a095aaca8d3a9eca9ba0264f4145ee1fd3c895` | Render command tests 16/0/0 and host sampling tests 5/0/0; default and ffmpeg desktop checks passed. New native host case completed 14 polls, 27 command records and 1024 output pixels across two actual requests | Command identity acceptance covers the top-level host exposure path. Decoder, surface presentation, nested-specific identities and final-source full regression remain pending |
| `53fd21d0b6d5691080b7d41aa2b0be2f2a1f050d` | Full render/host suites: 695 passed, 0 failed, 1 existing external-fixture ignore; ffmpeg desktop check passed | This is the exposure integration source, before the independent input and complete-trace increments |
| `da65b5dd76977ac2dd61b93c24eabbc3f14df719` | Focused render command 17/0/0, native input 8/0/0, host exposure 5/0/0, host input 1/0/0, desktop real file/console 1/0/0; eleven affirmative native reports; full ffmpeg desktop 856/0/3 and default/ffmpeg desktop checks passed | These input and metadata-sink results are not acceptance of the later complete-trace increment or native readback threshold-60 file output |
| `11c342d3a13b221a5007edcb31ca89bc4ad7b5c8` | Focused render command 21/0/0, native input 8/0/0, host exposure 5/0/0, host input 1/0/0, desktop real file/console 1/0/0; twelve affirmative native reports. The new actual trace case checked 266 native samples, 260 retained trace records and 64 pixels across ring eviction and a nonblocking stop | This source incorporates main `d07b8b13`. Its complete regressions are running; the later exact frame-link increment has not been compiled/tested |

The last source is based on merged main `81aca699`, with this branch's GPU commits reapplied. The earlier source `44de827f` failed 5/1/0 because cancellation cleanup called `unmap` after an actual cancelled mapping. Source `9aba5719` fixed that native cleanup defect without weakening the assertions. The original failure log and its disposition remain retained.

The new host fixture initially used a headless device without timestamp queries (`ff836269`): its 5/0/0 Rust count was explicitly rejected by the acceptance controller because the native report said unavailable. Enabling queries next exposed its incorrect linear target format (`88432ef1`, host 4/1/0). The final fixture uses the original exposure sRGB target contract with timestamp queries enabled. Both rejected attempts remain retained; no assertion was removed or weakened to accept `63a095aa`.

The first full exposure-integration attempt (`issue60-gpu-command-broad-v2`) terminated while compiling `allocation_budget`, before any test suite ran: Cargo exited 101 and rustc exited 1073807364. The cause is not established. That failed log and its disposition remain retained. Only the subsequent actual `broad-v3` run establishes the 695/0/1 result above; its full-suite log SHA-256 is `d32ae9e1d75650a4cb2cc7256253483ce9180e24104b9076c1ce18d1119b28cc`.

Local receipts are under `E:/Dev/aem/artifacts/pr-queue`: `issue60-gpu-command-marker-v1-*`, `issue60-gpu-command-primitive-v1-*`, `issue60-gpu-command-primitive-v2-*`, `issue60-gpu-command-broad-v1-*`, `issue60-gpu-command-integration-v1-*` and `issue60-gpu-exposure-identity-v1/v2/v3-*`. These new GPU records have not yet been published as an immutable archive or accepted by PR CI.

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
| G8: stable exposure identity | Requests receive nonwrapping process-unique IDs; every poll has a distinct request-local ordinal. Publish a bounded caller scope before preparation, including before any encoder exists. Command creation captures the identity independently of allowance frames; real callbacks retain it across pending polls and supersession. CPU scopes restore on exit/unwind; unknown/overflow scopes suppress the parent instead of falsely attributing work. Exhaustion remains unknown and never refuses/retries rendering |
| G9: independent input owners | Source `da65b5dd` explicitly opts each input device/queue owner into its own pool, preserving shared-budget admission, original uploads, queue writes and retirement. Native tests cover two actual devices, restart, admission refusal, exact pixels, old callbacks and owner destruction. Immutable input packet sequence/revision/object/frame context remains distinct from worker requests; oversized identifiers are omitted explicitly |
| G10: actual metadata fault sinks | Source `da65b5dd` runs the real desktop logger and actual Session guards in a child process at default Info. Healthy scopes remain quiet; metadata invariant faults defer until the guard exits, preserve the primary error, appear in the file and stderr, remain absent from stdout, suppress repeats in the epoch and report a new epoch once. This is a sticky metadata invariant, not native readback failure at occurrence 60 |
| G11: bounded recording window | Source `11c342d3` passed the focused unit/native cases. A separate explicit trace preserves terminal records beyond the 256-entry watchdog ring, capped at 8192 commands and 16 MiB of retained record-slot/string/context bytes. Creation boundaries use epoch and nonwrapping command/window IDs. Pre-window work stays excluded even when callbacks arrive late; stopping does not wait, fabricate completion or extend into later commands. Tests hit both actual production limits. Overflow preserves a prefix and counts omitted IDs; routine snapshots never clone the full trace |
| G12: exact presentation links (validation pending) | Successful normal and exposure FrameRecorder entries save the exact encoder epoch/id before native submission, plus the exposure request/poll when present. Finished buffers preserve that identity. Missing/disabled/busy observations stay null. This uses scalar Copy metadata, not a floating source-frame join or a growing per-frame context list. New tests cover duplicate numeric IDs across epochs, unchanged source frames, actual native callbacks and actual worker/surface/report joins; they have not yet run |
| G13: native threshold sinks (validation pending) | A new desktop child-process test drives 59/60/70 actual invalid native readbacks, a valid native render/pixel recovery, then a second 59/60 episode through actual Session guards and the default file/console logger. It checks retained caller requests, original primary failure, suppression, reset and stdout isolation. This test has not yet run; the metadata invariant sink case is not substituted for it |
| G14: explicit baseline trace integration (native validation pending) | The offline six-case collector now records original encoder words and the native queue period, exact main encoder identities, per-sample mapped diagnostic bytes and completed allocation windows. Its request domain is explicitly the offline case's sample index plus one, not a worker request or a floating source-frame join. Nested commands have a distinct stage; vector/MSAA preparation remains inside the main encoder, before root-pass timing. Seventeen Python protocol/archive tests passed, including five new rejection tests. This source has not been compiled or measured on a GPU; synthetic protocol records are not native evidence |

G13 uses a separate explicit diagnostic injector gated by both `debug_assertions` and `gpu-tests`; it is absent from Release builds and never invoked by normal rendering or profiling startup. It submits real query/resolve/copy commands followed by an actual GPU copy that corrupts diagnostic timestamp words. The real mapping callback and unchanged escalation/log path process that data. No successful/error callback result is fabricated on the CPU, no image or project data is modified, and no driver/hardware failure is claimed. Its admitted 16-byte source buffer retires on an actual queue fence, including submission unwind. This is test infrastructure, not a rendering optimization or automatic recovery mechanism.

Source `9c48a419` passed render/input/host focused cases 36/0/0, but its
desktop native-sink target failed to compile: the desktop `gpu-tests` feature did
not forward to the dependency, whose defaults are disabled. Its actual Cargo
artifact reported no render features. The later source explicitly forwards
`motion-render/gpu-tests`; no assertion, test selection or native callback result
is changed to accept this failure.

Source `aa06d1dd` then passed the two real default-file/stderr sink cases
(2/0/0), exact frame sequence cases (5/0/0), frame measurement cases (4/0/0)
and actual worker/surface presentation case (1/0/0). The native sink evidence
contains 130 actual invalid diagnostic readbacks, two valid native samples,
64 recovery pixels and two separately suppressed/reset Error episodes. The
presentation evidence joins four successful normal/exposure presentations to
their exact encoder identities and original query words. Neither result claims
a real hardware fault, scanout validation or steady-state performance.

That same run stopped before render unit tests: adding the native queue period
left a unit-only observer initializer incomplete (`E0063`). The later fix adds
the unit conversion constant to that metadata/log-scope fixture without changing
its assertions. The failed receipt remains retained; the successful desktop
cases do not turn this incomplete run into full regression acceptance.

The source-`11c342d3` full desktop run failed 877/1/3 in the existing composition
bundle child test. The same unchanged executable passed a direct isolated child
run once. Contemporaneous Windows access-violation events name the same test
executable, but the old fixture did not retain the child PID/status, so their
exact association and the crash root cause are not established. The original
full-suite failure remains retained. Later child assertions include exit status
for diagnosis; their success conditions remain unchanged. This is additional
failure evidence, not a claimed native crash fix.

Source `11c342d3` also renames command `cpuElapsedUs` to `elapsedUs`: it measures wall time through encoding, native queue and mapping completion. It must not be described as exclusive CPU preparation. Actual allocation-frame `cpuElapsedUs` retains its separate CPU-scope meaning. The accepted focused trace carries original words and exact input/exposure/allocation contexts. The new presentation links are a later increment with runtime acceptance still pending; neither increment claims full GPU coverage.

Source `63a095aa` also makes mixed-category failure-streak saturation sticky and visible; a successful sample resets the streak without erasing its historical saturation flag. The new scope metadata tests cover controlled blocking, bounds, unknown identity, out-of-order destruction, unwind, thread mismatch and exhaustion. The host native case verifies real ExposurePhase/ExposureResolve commands and exact request/poll/worker-window links; the existing receiver-pixel oracle and terminal-failure tests still pass. Allocation-frame contexts describe active shared-budget windows, not exclusive ownership or exclusive GPU execution. A presentation scope identifies the current attempt, including retained content, rather than identifying the image's originating request; surface presentation has not been accepted by this native case.

Native diagnostic corruption is an injected failure, not evidence of a hardware/driver fault. The six real cases cover independent intervals, cancellation/reuse, submission unwind, automatic active-frame association/restart, invalid-word escalation/recovery and the raw wrapper. They do not demonstrate every production scene route.

The ordinary root timer remains separate. Command intervals include marker overhead, root-query work inside the encoder, and possible backend scheduling gaps. Queue-generated writes before the first marker are outside them. Overlapping root and command intervals must not be added or described as exclusive GPU busy time.

## Pending work

- Extend the accepted top-level host exposure identities to remaining independent/export callers and validate nested/decoder/surface paths; do not infer that every independent ExposureFrame caller has a host request identity
- Preserve the completed independent PreviewInput regression's exact source attribution; it does not validate later changes
- Validate the new bounded full recording trace and extend exact presentation-frame links; the 256-record diagnostic ring alone cannot supply every earlier frame of a long report
- Verify actual native readback failure at occurrence 60 through the default desktop file and Error stderr sinks; the accepted metadata invariant sink case does not prove this separate threshold path
- Execute complete appropriate regressions for the final integrated source and actual native nested/vector/mask/effect/exposure/video/overlay paths, preserving exact source, quality, resolution and driver attribution
- Publish the new raw evidence and completion dispositions only after those gates pass; retain the existing application's allowance-pressure baseline without presenting it as physical/OS pressure acceptance

Issue #60 remains open. Neither compilation nor the focused test count closes these remaining requirements.

The new baseline writer emits schema 3. Historical schema-2 archives remain
replayable with their original source attribution and explicitly have no command
evidence. A fresh collection cannot publish schema 2. Its validation checks
original words against the queue timestamp period, exact epoch/encoder and caller
identities, actual callback byte counts, trace completeness/limits and completed
allocation windows. It rejects hidden omissions and unavailable data presented
as measured zero. Command intervals may overlap root timings and one another;
they are never summed into exclusive GPU busy time. Profiling overhead is part
of this new explicit study condition, so prior baselines cannot demonstrate an
optimization gain for this increment. Actual native collection remains pending.
