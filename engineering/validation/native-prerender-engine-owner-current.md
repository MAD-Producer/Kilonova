# Experimental Engine idle prerender owner (#82)

Accepted component source: `6cec3f9efa6ee7eac6cf8998d96a7e029daddd30`, based on main824054ff. Android is
excluded. This is an experimental idle-only High/authored-camera/single-time
profile. The general prerender capability remains false. Explicit playback
demand, external same-path source refresh, broader preview profiles, controlled
blocked decoder/Engine cancellation, pressure cases, paired foreground P95 and
combined full regression/cross-platform CI remain pending.

Disabled starts no background worker, GPU device or polling loop. Explicit
configuration owns the Coordinator on the existing Session thread with bounded
64 MiB CPU completion and 128 MiB native cache quotas. Dispatch cancels before
foreground enqueue. The shared generation prevents publication after a racing
dispatch, and published controls keep their slot until actual acknowledgment.
The independent observation API reads the native owner without queuing State.
Cancellation/invalid context is processed before completion installation.
Failures latch with their cause and do not automatically restart the native owner.

Internal polls have independent deadlines. Routine foreground State queries do
not reset that deadline, decrement queue depth or erase the retained front. Polls
are quiet; actual source capture and native jobs publish their starts. Present
keeps its existing start/done/dispatch filtering. Cache misses and unsupported
profiles use the ordinary render path. Exact native frames use the existing
Session output conversion, overlay, submit and retained-front commit.

The isolated actual-window control cold-renders red, waits for Engine-produced
native cache residency while repeatedly querying State, seeks and presents a
warm frame, and reads the committed retained front through the GPU Presenter.
Exact red pixels match cold output, and Session.cached_presented increases.
Editing the source to green must produce green while that counter stays unchanged.
This is retained-front pixel proof, not a physical monitor screenshot or P95 proof.

The v1 control failed because each State query restarted a relative 50ms wait,
starving idle work. The product now uses the remaining interval to an independent
poll deadline. The original assertions and deadline were retained; v1 failure
and v2 success are both archived. No existing assertions were changed.

## Evidence

- publication-gate: 2 passed, zero failures, 0 excluded
- native-engine-owner: 1 passed, zero failures, 0 excluded
- queue-regression: 4 passed, zero failures, 0 excluded
- diagnostics-regression: 12 passed, zero failures, 0 excluded
- desktop-check: passed
- provenance: passed

Archive: `data/native-prerender-engine-owner-v1.zip`.
SHA-256: `cf4bef7d9290092a36521533e863b50708d398215a86116f6d14aba5f64eaa36`. All 28 members were read back byte for byte.
The source-bound v2 receipt includes exact commands and selected/excluded counts.
Prior full Host/Render/media and four-precision checkpoints remain historical
source-bound evidence, not a full regression of this rebased Engine integration.
No disk-cache failure handling or complete #82 delivery is claimed.
