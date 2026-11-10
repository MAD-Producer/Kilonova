# Current-main integration of background prerender components

Issue [#82](https://github.com/MAD-Producer/Kilonova/issues/82) remains open. This is an integration record, not completion or a narrowed acceptance contract. Android remains outside the maintained desktop scope.

## Integrated corrections

- [Idle upload boundary](native-prerender-idle-upload-current.md): original native negative 0/1, corrected idle/playback 2/0, coordinator 17/0, media prerender 69/0/3 and original four-precision fusion 1/0. Playback retains deferred fusion; idle warming submits its own upload.
- [Input completion admission](native-prerender-input-completion-current.md): both actual pending-input and late-input interleaving negatives failed at their intended assertions, and fixed controls passed with real native pixels and recovery. The completed token's ordinary Present behavior remains unchanged.
- [Compact command queue](native-prerender-command-queue-stack-current.md): actual original main-stack overflow was reproduced and its immediate 62,968-byte channel probe identified. Original 20,960-byte queue-item control failed; boxed control and queue/observability regressions passed. Actual MCP/UI startup passed with the unchanged 1 MiB main stack. This does not diagnose earlier access violations or illegal instructions.

## Rebase and interface adaptation

All 81 prior commits were rebased from main `f2e9f0e82f625eb0e1095857bf3e98d917bcfbcf` onto current main `8573d9975cf70d2c8fe05605102ec9d744816070`. The old integrated tip is retained as `codex/82-before-main-8573`. Range-diff and the initial compile failure are retained in the local integration artifacts.

Two conflicts required semantic resolution. Independent view groups keep current main's native multiview presentation branches and bypass the single authored-view prerender cache. Test-only draw tracing starts before current main's view preparation, so the new preparation cost stays inside the measured draw interval. No panel files, layout, hit regions or styles were edited by this branch.

The first rebased test compilation failed with three E0063 errors: new UiFrame constructors in the prerender fixtures omitted current main's `views` field. The fixtures now explicitly specify `views: None`, retaining their existing single-view semantics and every original assertion. The preparation script's attempted automatic edit also stopped on an overly broad constructor-count assertion before writing any source; the subsequent compiler failure is retained. Neither failure is reported as acceptance.

The integrated code tip after fixture adaptation is `55b8429e`; later evidence-only commits may follow. Final source requires the original strict ABBA pairwise P95 criteria, actual native cache/input/storage controls, complete maintained workspace regression and MCP/UI verification. Completed pre-rebase components are evidence for their own source only.

## Outstanding acceptance

The most recent completed combined gate is still v8: 4,137 passed / 1 failed / 72 ignored, a strict property-input pair ratio of 1.0251986313286552, and failed shared MCP startup. The source-backed queue correction has a later successful MCP component gate; final integration still needs its own run. The original native effect-search access violation and other retained historical native abnormal exits remain unresolved. Physical constrained-device and exact final-head cross-platform limits remain explicit in the [acceptance map](native-prerender-issue82-acceptance-current.md).

Published head `652fbe65` had all five required CI jobs successful. Those checks do not qualify the new current-main integration. No old failure is erased, no threshold or existing assertion was weakened, and #82 is not ready to close merely because the component gates passed.
