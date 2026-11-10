# Native input and completed-result admission

Issue [#82](https://github.com/MAD-Producer/Kilonova/issues/82), base `652fbe6525f5813a85620fe0d5d2ad4eef5f9bfb`. These components passed actual native controls; they do not complete whole-application performance acceptance.

## Corrections

Runtime reconciles `Signals::input_pending()` before polling, alongside foreground queue occupancy. Input is published before action generation, so an empty serial queue does not imply that an optional upload may proceed.

A separate single-use `YieldControl` input marker closes the later boundary: input can arrive after demand reconciliation but before completed pixels are admitted. `Signals::input` marks the active token, and Coordinator rejects a marked completion. This preserves ordinary completed-token `preempt()` and `control().check()` behavior, native-owner reuse and strong cancellation. The existing input/foreground gates remain intact. No GPU work is performed under the signals lock; already-admitted/submitted asynchronous GPU work cannot be retracted.

## Actual controls

Input-completion v5 completed at `2026-10-31T08:20:48.697754+00:00`. Original production failed exactly 0 passed / 1 failed at the intended pending-input assertion. The same native fixture with the demand correction passed 1/0/0; original native routing, refresh and playback controls each passed 1/0/0.

Late-input native v5 completed at `2026-10-31T12:49:00.763215+00:00`. With the demand correction but without the input marker, the new late-interleaving control failed exactly 0/1/0 at `late input must reject completed optional pixels after demand reconciliation`. With the marker it passed 1/0/0. The original completed-input control, native input routing and native playback each passed 1/0/0; signals regression passed 5/0/0. The five worker yielding controls (including four actual native controls) and coordinator regression passed. Provenance, inventory and whitespace checks passed.

Both native fixtures use a real Engine surface, Session, background worker, completed native pixels, zero optional upload/submission admission while input is pending, cancellation acknowledgment, released CPU/publication leases, and exact red pixels after recovery. Recovery acknowledgment in this focused fixture invokes the protocol directly; the separate routing control verifies actual foreground Present.

The fixture waits for actual preparation/render mailbox acknowledgments. `completion_waiting` is published before mailbox send and alone cannot prove that a completion can be consumed. Original assertions were retained while correcting this new fixture's synchronization. Input v4's preparation failure is retained, not accepted. Late-native v4 executed no tests and made no source edits: it captured the predecessor's temporary original-production file during its negative control, and rejected the subsequently restored accepted source. The only snapshot difference was `engine/prerender.rs`. Late v5 captured the stable accepted source before first execution.

Seven extracted production-method CPU marker controls also passed. They deliberately control the completion flag and do not replace actual Signals/GPU/native testing.

## Evidence and limits

Archive [native-prerender-input-completion-controls.zip](data/native-prerender-input-completion-controls.zip), SHA-256 `ae6a18b91040c90edf90ce51167e4b3e19175a355cb90418a71e82f810097b5b`, 64 read-back-verified members. It retains original negative results, corrected positive results, earlier failures, sources and logs; it excludes native binaries, PDBs, private process dumps and WER reports.

Original combined-v8 strict property-input pair 1 ratio 1.0251986313286552 failed. Its full workspace had 4137 passed / 1 failed / 72 ignored, including an unresolved native effect-search access violation; shared MCP startup overflowed the main stack. These component passes do not erase those failures or prove their causes. Final integrated source still requires unchanged strict input/playback criteria, complete maintained workspace regression, actual MCP startup and exact-head CI. Android is not maintained; no panel files changed.
