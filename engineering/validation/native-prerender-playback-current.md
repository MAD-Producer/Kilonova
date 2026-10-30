# Native playback prerender routing checkpoint (#82)

Accepted source: `b349edb26ed5e0402783d35fa7c889f0412fa74d`. Android is excluded. This adds experimental
High, authored-camera, single-time playback demand to the prior idle profile.
The general prerender capability remains false; the default policy is disabled.
This component checkpoint does not close #82 or accept the whole 3D route.

Shell playback actions, clock advances, presentation and stop publish explicit
playback state through the short dispatch gate. Repeated state is a no-op. A
state change invalidates an older publication generation; it cannot admit a job
that raced foreground demand. Present and frame seeks yield running jobs while
preserving completed single-use native results. Edits, reconfiguration and
shutdown still strongly cancel, including after completion publication.

The additive playback scheduler retains a ticket only on continuous motion with
the same full context, direction, frame range and exact fractional grid inside
the new bounded window. Fast seeks, reverse motion, output/content/document/device
changes, invalid demand and foreground/edit/pressure holds still cancel. The
original explicit-seek scheduler API remains conservative. Retained completion
keys are pruned as the window moves.

After live signature/profile/pressure checks, the foreground can drain a completed
yielded frame due now before cache lookup. It does not drain a different future
job merely to read the current frame. Actual native coordinator controls verify
exact red pixels after playback advances and rejection after strong cancellation,
pressure and generation invalidation. Existing cancellation assertions are intact.

The actual hidden-window Engine test uses a 3600-second idle delay and a 30-second
warmup deadline to distinguish explicit playback demand from idle work. It warms
two native frames, queues Seek and Present behind a controlled serial gate, and
reads the successfully committed retained front: exact cold/warm pixels and one
actual cached presentation. This controlled queue establishes foreground priority;
it is not a paced playback or latency benchmark. A separate real dispatch/native
worker control distinguishes completed Present yield from editing invalidation.
The prior idle warm/edit and double-blocked native decoder controls reran.

## Exact-source results

- publication-gate: 4 passed, 0 failed, 0 ignored
- native-playback: 1 passed, 0 failed, 0 ignored
- native-idle: 1 passed, 0 failed, 0 ignored
- native-blocked: 1 passed, 0 failed, 0 ignored
- shell-playback: 1 passed, 0 failed, 0 ignored
- queue-regression: 4 passed, 0 failed, 0 ignored
- diagnostics-regression: 12 passed, 0 failed, 0 ignored
- host-regression: 265 passed, 0 failed, 1 ignored
- media-regression: 96 passed, 0 failed, 2 ignored
- desktop-check: passed
- provenance: passed

Focused controls overlap the full Host/media suites; counts are not added as
unique tests. Every ignored test is listed individually in accepted.json. Full
desktop and Renderer regression are not claimed by this checkpoint. The v1 run
lost its process handle and had no live owner, no receipt and empty first-step
logs; it is archived as interrupted, never as acceptance. V2 passed 24 selected
desktop tests and 139 Host unit tests, then the asset_registration integration
process aborted at an unsafe pointer precondition in Rust core. Its original logs
and exit status are retained. The individual failing test and the unchanged
eight-test integration suite both passed in diagnostic reruns; the intermittent
abort root cause remains unconfirmed and is not claimed fixed. V3 reruns the
entire checkpoint with full backtraces enabled and uses separate output.
No existing assertions were weakened. No panel, layout or style file changed.

## Remaining limits

Managed revisions and explicit refresh do not detect arbitrary external same-path
file replacement. Auto/reduced/observer/exposure output still uses the normal
foreground fallback. Running-job yield can retire the background source and close
lookup until a matching frozen-source acknowledgment; the retained cache is not
automatically proof of live validity. Foreground reservation can evict a future
cache entry, so controlled warm-hit evidence does not establish sustained hit rate.
Paired cold/warm P95, low-resource controls, broader output profiles, reopen/source
invalidation controls, fresh-main combined full regression and cross-platform CI
remain required. No optional disk cache or disk-full/corruption proof is provided.

Archive: `data/native-prerender-playback-v1.zip`.
SHA-256: `a88e320b8ae8d32220e5180e13a8803e1ba2da23f09e849a83fd3f21e80afc1f`. All 73 members were read back byte for byte.
