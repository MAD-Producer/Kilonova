# Background composition prerendering (#82)

Status: owner-side scheduling primitive implemented and tested; actual prerender
render/cache integration is not implemented or accepted. This work remains
part of the active delivery alongside the #87 depth/export work. Android is
outside the maintained scope. No UI behavior change is authorized by this note.

## Current evidence

- #82 is open and requests local, cancelable composition prerendering with
  foreground priority, content/versioned cache keys, bounded memory/optional
  disk storage, generation-safe installation and observable disabled/idle states
- #61 is closed; its shared resource admission and preview pressure policies
  must also constrain prerender targets and work in flight
- #65 (final composition frame cache), #71 (bounded immutable preparation jobs)
  and #79 (DAG/group result versions and caching) are still open. None is counted
  as delivered merely because image decode/lookahead or geometry plan reuse exist
- `Session::render_with_view` samples and updates mutable preview state on the
  Session owner. `present_retained_with_view` can repaint chrome over the one
  held completed frame; it is not a multi-frame final composition cache
- Video `FrameCache` stores decoded source samples. Panel frame caches retain UI
  layout. Neither is a prerendered composition cache
- `ops::snapshot` currently reports `capabilities.prerender = false`. It must
  stay false until the actual render/install path and cancellation are verified
- The inspected open branches/PRs have no #82 scheduler implementation. The
  image active-set work in #548 is separate renderer allocation work and must
  not be overwritten or claimed as this delivery

## Implementation and proof obligations

1. Add a final-composition-only cache and key construction covering the exact
   fractional frame/PTS, immutable project/composition, source/plugin/model
   content, expression/camera/mask/matte state, output format/color/quality and
   device generation. Chrome/selection overlays remain outside cached pixels.
   Unknown temporal/external dependencies receive conservative exact-frame keys.
2. Schedule a bounded directional window from immutable inputs. Seeking/editing,
   foreground demand, document/source generation changes, pressure or disabling
   must cancel old work and reject any late installation. Session/decoder/GPU
   thread ownership must remain valid; a second unconstrained renderer is not
   a substitute for shared budget admission or foreground priority.
3. Publish starts/in-flight phases before blocking preparation/IO/render work,
   along with bounded hit/miss/eviction/bytes/queue/cancel/stale counters and
   the reason work is held or disabled. Keep healthy Present paths quiet.
4. Add optional disposable disk storage with explicit directory/quota, versioned
   working-format payloads, integrity checks and atomic publication. Full disks
   and corrupt entries discard only reproducible cache data; original projects
   and formal export destinations are never cache eviction targets.
5. Verify warm versus cold intervals using actual rendered pixels, cache counters
   and peak allocations. Separately measure paired foreground P95 and work
   cancellation during real scrubbing/editing, including low-budget/disabled
   operation. Passing scheduling unit tests alone cannot close this issue.

Required functional cases include reverse playback, rapid/repeated seeks,
fractional frames, static and time-dependent scenes, alpha, precision/quality
changes, undo/redo, same-name source replacement, concurrent editing, switching
projects, stale completions, full/corrupt disk caches and GPU consumers still
holding evicted slots. Network rendering remains outside #82's local delivery.

## Related ownership work

The current #87 scalar spill/frozen snapshot work supplies a bounded ownership
primitive for immutable depth inputs. Its five-second real-codec tests do not
prove a prerender scheduler, final frame cache, foreground P95 or sustained
playback, and are not counted as #82 completion.

## Implemented scheduling primitive

`motion_host::prerender` holds at most eight exact fractional frame keys and one
worker ticket. It selects forward/reverse windows, waits for an injected idle
threshold when stopped, suppresses fast seeks, and publishes hold reasons for
foreground demand, editing, pressure, disabled policy and admission refusal.
Counters saturate over the Scheduler lifetime; invalidation does not reset them.

Content, output, document and device generations are explicit ticket inputs.
They are caller contracts, not an implemented authoritative project hash factory.
Changes atomically cancel the old ticket. The single in-flight slot remains
occupied until its worker acknowledges completion: repeated seeking cannot
silently start unlimited replacement jobs. A late or foreign ticket cannot
clear the new flight. Dropping the owner publishes cooperative cancellation.

Source-bound evidence: `artifacts/pr-queue/prerender-scheduler-v2/accepted.json`.
Twelve standalone Rust tests passed, including a controlled blocked worker.
A deliberately faulty copy that releases its canceled slot early fails the
regression guard. The original product sources remain unchanged by this check.
No old assertions were modified. Cargo crate integration is still pending.

This pure policy performs no rendering, IO or GPU reservation and creates no
worker thread. Its admission boolean must come from an actual shared-budget
reservation held by the future consumer. It does not prove runtime foreground
priority, preemption latency, P95, final-frame hits or disk resilience. Static
interval proof, content-key construction, bounded immutable preparation, actual
render/install integration, caches and real measurements remain pending above.
`capabilities.prerender` remains false. No UI, dependency or asset changes apply.

[Standalone scheduling evidence](data/prerender-scheduler-native-v1.zip), SHA-256:
`b8b3b187c669c895dbed3a60c43df98adf41afeba7e5d9cb7843fb1c212a97c3`

All 14 entries were read back and verified. This archive excludes build binaries.
