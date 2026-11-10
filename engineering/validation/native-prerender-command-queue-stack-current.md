# Compact engine command queue and native startup

Issue [#82](https://github.com/MAD-Producer/Kilonova/issues/82). Source base `380bbbb72eba919b47aaaa71536401687dacb4a9`; component gate completed `2026-10-31T12:56:53.237012+00:00`.

## Actual failure and correction

Combined-v8 shared MCP startup failed with main-thread stack overflow. One diagnostic execution of its preserved application reproduced first- and second-chance `c00000fd` and captured both actual exception dumps. The stopped instruction was the owned CRT stack probe, requesting 62,968 bytes. Its saved-register prologue identifies the return address at fault RSP+16. The owned PE runtime-function entry and caller disassembly show a channel prologue loading `0xf5f8` and calling the probe. This identifies the immediate failing allocation; stack-word scanning is not presented as a fully unwound call chain.

`QueuedCommand` occupied 20,960 bytes and was passed by value through the bounded channel. The channel now moves a boxed command. Allocation occurs before the dispatch gate, so the gate closure also captures a pointer; enqueue timing is still assigned under the gate. The worker unboxes accepted commands, while synthetic prerender polling remains stack-local. Capacity, backpressure, shutdown draining, command ownership and reply behavior are preserved. This adds one heap allocation per dispatched command; integrated timing must include that cost.

The application's default 1 MiB main-stack reserve is unchanged (actual fixed PE readback: 1048576 bytes). No thread stack enlargement, native-layer disabling, retry or command restart was used.

## Actual controls

The same new channel-layout and blocked-reply test failed against original production exactly 0 passed / 1 failed, retaining measured `item_bytes=20960 payload_bytes=20960`. With the correction it passed 1/0/0. Original queue regression passed 5/0/0; blocked-worker observability and dispatch-lock observability each passed 1/0/0; prerender runtime regression passed 7/0/9. The application built and actual shared-engine MCP verification passed, including real protocol and UI activity. Provenance, inventory and whitespace checks passed. These are overlapping executions, not additive unique coverage.

## Evidence and remaining limits

Archive [native-prerender-command-queue-stack-controls.zip](data/native-prerender-command-queue-stack-controls.zip), SHA-256 `9be0f5c997c11734ed9a0e4e5a6c167b13189157f38e0a0bbfd000267c5de05f`, 31 read-back-verified members. It contains source, actual negative/positive logs, startup logs, sanitized original exception facts and owned caller disassembly. Private process dumps, binaries, PDBs and WER reports are excluded.

This addresses the reproduced command-channel stack-pressure/startup path. It does not establish causes or fixes for earlier native effect-search access violations, illegal instructions or other retained abnormal exits. Original combined-v8 strict property-input P95 ratio 1.0251986313286552 remains failed. Final integrated source must incorporate the input-admission components, rebase onto current main and pass unchanged strict performance/full workspace/MCP/exact-head CI gates. Android is out of maintained scope; no panel files changed.
