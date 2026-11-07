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

The final ordinary native v2 command did not reach any of the41 tests. Its
`rustc.exe` compiling motion-host exited with `0xc0000005` (Cargo101, controller1).
Every observed source byte remained unchanged. The cause is unknown. The earlier
41-pass run predates the flush correction and does not qualify final native
acceptance. The corrected actual file probe separately built and passed. No
compiler diagnostic, old green result or unrelated full pass substitutes for a
new ordinary final command.

The [boundary manifest](data/depth75-aligned-boundaries-v1.json) and
[archive](data/depth75-aligned-boundaries-v1.zip) retain the actual failed final
command, raw stdout/stderr, source delta, original failed file probe, corrected
passing file probe and consumed terminal records. Final native and cross-platform
qualification remain pending. The success-only archive builder is unexecuted.

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
