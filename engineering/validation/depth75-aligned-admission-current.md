# Depth transition aligned-result admission

Issue #75 remains open. This increment starts from main `32d3c860`, after #629.
It connects the existing prepared worker result, alignment proof and transition
generation into an owned admission object. It does not install it into Session,
choose a quality tier, create a persistent cache or freeze an export policy.

`transition::admission::Admission::prepare` requires the candidate's current
ticket/live identity and an actual prepared reference. The authoritative prepared
RGB fingerprints must match: equal source/model metadata alone does not prove
equal pixels. A stale candidate returns a normal miss before fitting. Mismatched
pixels return an explicit Error; fit rejection and cancellation retain their
original typed alignment causes. Every alignment limit remains caller supplied.

The object retains one reference resource instance, one prepared candidate and
their instance-bound proof. It creates no global cache, second scalar plane or
worker. Raw values and model metadata remain unchanged. Its view rechecks the
controller owner/generation against independently read live identity, rejects
seeks/invalidation/ABA/foreign controllers, and borrows the controller for the
view's lifetime. Samples use the aligned mapping; metadata, provider, costs and
prepared-input identity retain the actual candidate provenance. This per-object
bound is not an application-wide memory quota or persistent cache identity.

Five new actual-Jobs controls check known affine samples without raw rewriting,
provider/cost/pixel identity, different pixels under identical metadata, stale
work before fitting, post-fit invalidation/foreign ownership/live time, original
cancellation and explicit scale refusal. They run alongside the unchanged
fourteen alignment, nine transition, seven Jobs and six routing controls.

A production-sink blocking probe separately checks the actual default Info file
before releasing alignment. Its first execution failed: the new Info line was
buffered until teardown despite all41 mechanism controls passing. The original
process101/controller1 outputs are retained. Flushing immediately before fitting
made the identical probe pass. A real pixel-mismatch Error then appears in file
and stderr; stdout contains exactly one JSON result. Cancellation is not logged
as a fault. Fit refusals retain their reason at Warn; no error is swallowed or
converted to a successful installation.

The first final ordinary native v2 command did not reach any of the41 tests. Its
`rustc.exe` compiling motion-host exited with `0xc0000005` (Cargo101, controller1).
Every observed source byte remained unchanged. The cause is unknown. The earlier
41-pass run predates the flush correction and does not qualify final native
acceptance. The corrected actual file probe separately built and passed. No
compiler diagnostic, old green result or unrelated full pass substitutes for a
new ordinary final command.

An owned compiler observation subsequently completed41 tests without an
exception. Debugger timing and compilation metadata differ, so this neither
explains nor repairs the original access violation. A separate ordinary command
at committed `522bd1ca`, without observer or wrapper, then passed all41 controls.
Its actual terminal was consumed with exit0. The previous failure remains an
unexplained compiler failure, not a failed assertion or an inferred memory cause.

The [boundary manifest](data/depth75-aligned-boundaries-v1.json) and
[archive](data/depth75-aligned-boundaries-v1.zip) retain the actual failed final
command, raw stdout/stderr, source delta, original failed file probe, corrected
passing file probe and consumed terminal records. The
[ordinary qualification manifest](data/depth75-aligned-admission-v2.json) and
[archive](data/depth75-aligned-admission-v2.zip) add the separate ordinary41 pass
and bounded diagnostic, retaining the original failures. Every archive entry was
read back:1,052,054 bytes, SHA-256
`a6430f3d7b5a23a9fa12d00ee2b9905013e7c628a132a006c9d696fc0e550877`.

The follow-on source delta explicitly connects alignment to the existing Jobs
worker. `Controller::request_aligned` retains one immutable prepared reference
and caller-supplied limits with the request. The same thread performs inference
and fitting, publishing `phase=alignment` before the latter. Cancellation is
polled by the existing fit and checked again before publication. Only the one
running, one replaceable pending and one ready slot are retained. This structural
bound does not supply the application's eventual retention quota.

`take_aligned_ready` performs no fitting. It checks the exact worker/request and
owner/generation against independent live identity; the returned view checks
again before sampling. Raw and aligned completions are distinct internally.
Using the wrong result API returns an explicit typed error and consumes that
completion; it cannot silently strip the proof and install raw output. Existing
raw image, video and archive requests retain their behavior. Provider, model,
input fingerprint and raw samples are preserved. `Costs.alignment` measures the
fit separately; worker total includes it. There is no new thread, implicit retry,
fallback, restart or product timeout. Session does not submit this path yet.

Seven additional actual-worker controls cover the same worker thread, original
provenance and measured costs; stale/foreign/wrong-input requests; original
pixel/scale failures; in-flight publication before a controlled alignment block;
cancellation with a newer queued request; explicit raw/aligned API boundaries;
and preservation of a real backend cause without retry. The final source delta
passed all48 targeted controls and the complete motion-host command:

`cargo test --locked -p motion-host --features ai-depth --no-fail-fast -- --test-threads=1`

Actual result:443 passed,0 failed,0 ignored,0 filtered; all28 integration targets,
library tests and doctests were accounted for. Default GPU execution and existing
assertions were preserved. This is a host regression, not a fresh complete
workspace pass or cross-platform qualification of this follow-on delta.

A separate production-sink probe found another real pre-return gap: the worker's
Info inference start remained buffered while its backend was blocked. The probe
failed with process101 before release. Flushing that start before preparation
made the identical behavioral fixture pass, alongside actual alignment output
and a pixel-mismatch Error with request/source/phase/cause in file and stderr.
Stdout contains one JSON result. Its earlier new probe setup failed to compile
because a stub type lacked its namespace; no behavioral assertion ran in that
attempt. That setup failure is also retained. Cancellation and superseded work
remain normal branches; refusal to strip a proof is a functional API boundary.

The [worker manifest](data/depth75-worker-alignment-v1.json) and
[archive](data/depth75-worker-alignment-v1.zip) retain source-bound final48,
complete host443, actual blocked-file RED/GREEN, original setup failure,
fixtures, original stdout/stderr and consumed terminal records. All44 members
were read back:680,418 bytes, SHA-256
`d1ecaca3e0a187d1cb2808d8e8bc0e6448475da986f80a4fe323e7efce14a4f0`.

Cross-platform CI for the preceding committed `522bd1ca` is separately tracked
by run42354329634. Linux completed successfully at the last verified response;
the workflow was still in progress at the verified08:45:39Z response. Earlier
GitHub API reads failed with TLS handshake timeouts, which are observation
failures, not CI failures. Neither that partial status nor earlier green heads
qualifies the follow-on worker source delta.

The preceding #629 source `15067fc5` separately passed all36 exact named controls
on Linux, Windows and macOS Intel:108 named passes. Its
[CI archive](data/depth75-routing-ci-three-v1.zip) and
[manifest](data/depth75-routing-ci-three-v1.json) retain the three raw logs.
The [mixed-backend archive](data/depth75-mixed-execution-v1.zip) and
[manifest](data/depth75-mixed-execution-v1.json) retain actual CPU266 -> Vulkan518
-> CPU266 frozen-reference execution at that source, including the original
observer preflight failure. These qualify the earlier source, not this delta.

Still required for #75: actual Session installation and versioned cache policy,
measured capability/quality selection with requested/resolved identities, frozen
export policy, continuous real-video drift/jump controls, sustained/thermal and
other supported desktop hardware. Numerical agreement and synthetic affine
controls do not establish scene quality or a real-time guarantee. No panels/UI,
Android, dependency, default model distribution or project format was changed.
