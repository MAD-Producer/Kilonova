# Issue 82 acceptance map

Current integration: see [main-8573 integration](native-prerender-integration-main-8573-current.md). The latest completed combined result below remains failed; later component passes and rebasing do not qualify the final source.

Issue: [#82](https://github.com/MAD-Producer/Kilonova/issues/82).
Delivery: [draft PR #575](https://github.com/MAD-Producer/Kilonova/pull/575).
This is an acceptance index, not a completion declaration. It maps the five
implementation requirements and three acceptance requirements in the issue to
their actual evidence. Android is retired and excluded by user instruction.
Issue #87 is independently [delivered and release-qualified](scene3d-main-release-current.md).

The latest completed combined gate tested `652fbe6525f5813a85620fe0d5d2ad4eef5f9bfb`
on main `f2e9f0e82f625eb0e1095857bf3e98d917bcfbcf`. Its full regression,
strict input P95 and actual MCP startup failed. All five required PR CI checks
passed on that same published head; CI does not override local acceptance.
Later upload/input-admission corrections and diagnostic-message changes need
qualification on their final integrated source. Historical reports describe
their own boundaries; their old remaining-work lists are not current status.

## Original requirements

| ID | Original requirement | Current evidence and disposition |
| --- | --- | --- |
| S1 | Select a small window using playback direction, seek speed, static intervals and idle state; prioritize and cancel for foreground demand | Implemented. Current-source `host-scheduling` and native priority/static/gap/refresh/blocked/idle controls passed in combined v8. [Input admission](native-prerender-input-priority-current.md) and [playback routing](native-prerender-playback-current.md) explain the protocol. Foreground latency acceptance A1 remains failed; cancellation correctness alone does not satisfy it |
| S2 | Use content, format/color/quality versions and quotas for memory and optional disk caches without contaminating formal output | Implemented with four native precision layouts, ownership admission and optional application-owned disk storage. Current-source storage, worker, foreground-installation and explicit four-precision controls passed. See [disk scope and bounds](native-prerender-disk-cache-current.md). These component passes are not final integration acceptance |
| S3 | Avoid competing with foreground GPU/decoder/memory/power on constrained devices; expose disabling and observable state | Default disabled, bounded CPU/GPU ownership, cooperative input preemption, pressure holds and independent in-flight observation are implemented and controlled. Current-source pressure and blocked-owner controls passed. [Public policy](native-prerender-public-controls-current.md) and [native observation](native-prerender-engine-controls-current.md) document these boundaries. Physical low-memory/power behavior and arbitrary hardware are not qualified; the measured foreground regression remains open |
| S4 | Stop stale generations on project/edit/source changes and install only matching completions | Current-source source-change, foreground-refresh, pressure/reopen and native precision controls passed. [Source freshness](native-prerender-source-freshness-current.md) covers live filesystem invalidation independently of the serial Engine queue. Watcher delivery and already-submitted GPU work retain their documented asynchronous limits |
| S5 | Deliver local prerender first; extend network rendering separately after frozen-job and recovery protocols exist | Local-only implementation. No network renderer or remote-filesystem guarantee is claimed. This preserves the issue's explicit separation of future network work |
| A1 | Cancel promptly when dragging/editing and do not worsen foreground P95 | Cancellation protocol controls pass, but strict native input P95 **fails** on current source. The first property pair is `1.0251986313286552`; the second is `0.9430862064081136`, and pooled property ratio is `0.9954186963581536`. An improving pool does not override a failed pair. Both timeline pairs pass. All 480 samples were verified; no sample, pair or threshold was changed. Finer draw-stage attribution completed as a diagnostic; its original strict assessment also failed, and it is not a production fix |
| A2 | Full/corrupt cache loses only regenerable cache and does not damage projects | Corruption/quota/partial-write and real worker fallback controls passed. [Actual Linux bounded-full-volume recovery](native-prerender-full-volume-current.md) passed on this exact source, including existing hits, saved-project preservation/reload and recovery after releasing only owned filler. Windows/macOS physical-full-volume behavior and storage hardware durability remain unqualified |
| A3 | Compare unwarmed and warmed intervals using actual cache hits rather than claiming zero-wait playback | Current-source paced study completed 240 frames with two enabled episodes of 60 actual hits each and zero missed-next-deadline samples. Strict pair ratios `0.8562281019693125` and `0.7650231124807396` passed. This is native Engine playback evidence, not whole-app input acceptance or a universal hardware guarantee. The [paired protocol](native-prerender-paced-protocol.md) remains unchanged |

The experimental native producer explicitly supports High, authored-camera,
single-time output. Other profiles retain foreground rendering. Disabled or
headless policy does not silently create native background work. These scope
limits must stay visible in the delivered controls and completion record.

## Integration gates and retained failures

- [Exact-head PR CI](native-prerender-ci-head-current.md) passed all five required
  checks on `652fbe65`. PR policy excludes arm64 release, FFmpeg packaging and
  publication; this is not four-platform release qualification
- The [combined v8 gate](native-prerender-combined-gate-current.md) completed
  34 steps, with 31 passed and three failed. Full workspace: 4,137 passed,
  one failed and 72 ignored. Desktop subset: 1,515/1/59. Actual MCP startup
  separately failed with main-thread stack overflow. Strict input P95 failed
- The original native effect-search access violation remains unresolved. One
  same-binary diagnostic passed; it did not explain or fix the original failure
- The [v7](native-prerender-combined-v7-failed.md) and
  [v6](native-prerender-combined-v6-failed.md) failures remain retained. Later
  changed-source or different-machine passes do not establish their causes
- [Child failure diagnostics](native-prerender-child-failure-evidence-current.md)
  preserve actual ExitStatus beside the original output without changing the
  success predicate. Standalone subprocess controls do not replace full regression
- Idle upload submission and pending/late input admission corrections require
  their real native controls, original whole-app P95, final full regression,
  actual shared-window MCP and exact-head CI. The original requirements above
  remain in scope; accepted components do not complete the issue

The complete original v8 public evidence is archived in the linked combined
report; private crash binaries/symbols/records are excluded. PR #575 remains a
draft and #82 open. Android validation remains excluded by user instruction.
