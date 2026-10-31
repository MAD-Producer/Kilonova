# Issue 82 acceptance map

Issue: [#82](https://github.com/MAD-Producer/Kilonova/issues/82).
Delivery: [draft PR #575](https://github.com/MAD-Producer/Kilonova/pull/575).
This is an acceptance index, not a completion declaration. It maps the five
implementation requirements and three acceptance requirements in the issue to
their actual evidence. Android is retired and excluded by user instruction.
Issue #87 is independently [delivered and release-qualified](scene3d-main-release-current.md).

The current integrated implementation is `8bb44d4a81f67f693a20e7bc3c067a987d37cb31`,
based on main `49890ce0da7d7701eaec5e521d749b562d3ffd47`. Later local launcher
and evidence commits are not covered by its CI. Historical component reports
describe their own source boundaries; their old remaining-work lists are not
the latest issue-level status.

## Original requirements

| ID | Original requirement | Current evidence and disposition |
| --- | --- | --- |
| S1 | Select a small window using playback direction, seek speed, static intervals and idle state; prioritize and cancel for foreground demand | Implemented. Current-source `host-scheduling` and native priority/static/gap/refresh/blocked/idle controls passed in combined v7. [Input admission](native-prerender-input-priority-current.md) and [playback routing](native-prerender-playback-current.md) explain the protocol. Foreground latency acceptance A1 remains failed; cancellation correctness alone does not satisfy it |
| S2 | Use content, format/color/quality versions and quotas for memory and optional disk caches without contaminating formal output | Implemented with four native precision layouts, ownership admission and optional application-owned disk storage. Current-source storage, worker, foreground-installation and explicit four-precision controls passed. See [disk scope and bounds](native-prerender-disk-cache-current.md). These component passes are not final integration acceptance |
| S3 | Avoid competing with foreground GPU/decoder/memory/power on constrained devices; expose disabling and observable state | Default disabled, bounded CPU/GPU ownership, cooperative input preemption, pressure holds and independent in-flight observation are implemented and controlled. Current-source pressure and blocked-owner controls passed. [Public policy](native-prerender-public-controls-current.md) and [native observation](native-prerender-engine-controls-current.md) document these boundaries. Physical low-memory/power behavior and arbitrary hardware are not qualified; the measured foreground regression remains open |
| S4 | Stop stale generations on project/edit/source changes and install only matching completions | Current-source source-change, foreground-refresh, pressure/reopen and native precision controls passed. [Source freshness](native-prerender-source-freshness-current.md) covers live filesystem invalidation independently of the serial Engine queue. Watcher delivery and already-submitted GPU work retain their documented asynchronous limits |
| S5 | Deliver local prerender first; extend network rendering separately after frozen-job and recovery protocols exist | Local-only implementation. No network renderer or remote-filesystem guarantee is claimed. This preserves the issue's explicit separation of future network work |
| A1 | Cancel promptly when dragging/editing and do not worsen foreground P95 | Cancellation protocol controls pass, but strict native input P95 **fails** on current source. The second property pair is `1.1349866349866349` and pooled property ratio is `1.0183869406881096`. Both timeline pairs pass. All 480 samples were verified; no sample, pair or threshold was changed. Finer draw-stage attribution is a queued diagnostic, not a production fix |
| A2 | Full/corrupt cache loses only regenerable cache and does not damage projects | Corruption/quota/partial-write and real worker fallback controls passed. [Actual Linux bounded-full-volume recovery](native-prerender-full-volume-current.md) passed on this exact source, including existing hits, saved-project preservation/reload and recovery after releasing only owned filler. Windows/macOS physical-full-volume behavior and storage hardware durability remain unqualified |
| A3 | Compare unwarmed and warmed intervals using actual cache hits rather than claiming zero-wait playback | Current-source paced study completed 240 frames with two enabled episodes of 60 actual hits each and zero missed-next-deadline samples. Strict pair ratios `0.9729916897506925` and `0.8336580613735997` passed. This is native Engine playback evidence, not whole-app input acceptance or a universal hardware guarantee. The [paired protocol](native-prerender-paced-protocol.md) remains unchanged |

The experimental native producer explicitly supports High, authored-camera,
single-time output. Other profiles retain foreground rendering. Disabled or
headless policy does not silently create native background work. These scope
limits must stay visible in the delivered controls and completion record.

## Integration gates and retained failures

- [Exact-head PR CI](native-prerender-ci-head-current.md) passed all five required
  checks on `8bb44d4a`: Rust/GPU, dependency audit and Linux/Windows/macOS Intel
  desktop. PR policy excludes arm64 release, FFmpeg packaging and publication;
  this is not four-platform release qualification
- The local full FFmpeg/AI-depth workspace gate is still running. Its completed
  desktop subset reports 1,463 passed, zero failed and 51 ignored. No whole
  workspace acceptance is inferred from that subset or from its live log
- The [older combined v6 gate](native-prerender-combined-gate-current.md) remains
  failed. Illegal instruction, stack overflow, Naga precondition and nonzero
  child-exit evidence retain unresolved causes. Later changed-source passes do
  not establish that these failures were fixed
- Integrate the local launcher/evidence repairs and any measured performance
  correction, align with current main, then qualify that final source with full
  regression, actual shared-window MCP and exact-head cross-platform CI

The current combined-v7 terminal receipt and diagnostic artifacts have not yet
been archived as final evidence. Its input failure remains a failure even if
all later independent checks pass. PR #575 must remain a draft and #82 open
until the outstanding acceptance and abnormal-exit dispositions are resolved.
