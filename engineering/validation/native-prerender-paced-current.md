# Paired native model playback (#82)

Accepted local component source: `7468f7c00c237870eeadcc6dd97548771564abb0` on main 55496217.
The test and fixed criteria are in `native-prerender-paced-protocol.md`. Android
is retired. General prerender capability remains false; this does not close #82.

## Actual result

The explicit ignored native test passed 1/0/0 and its independent versioned raw
evidence analyzer accepted the unchanged paired criteria. Four fresh Engines ran
disabled/enabled/enabled/disabled episodes, each presenting 60 frames at 30fps in a
640x360 native window with an owned animated OBJ triangle, High authored-camera
single-time rendering. Enable/disable went through the public MCP ToolRouter.
All final retained-front pixels matched exactly and the authored project was
unchanged. Foreground Seek and successful Present were timed; per-frame State and
one final readback were outside that interval. No per-frame PNG capture occurred.

| Episode | Prerender | Actual cached presentations | Foreground P95 | Deadline misses |
|---|---|---|---|---|
| 0 | Disabled | 0/60 | 10533us | 0 |
| 1 | Enabled | 60/60 | 7904us | 0 |
| 2 | Enabled | 60/60 | 7833us | 0 |
| 3 | Disabled | 0/60 | 8059us | 0 |

Enabled/disabled P95 ratios were 0.7504034938 and 0.9719568185 for the two fixed
pairs. Pooled disabled P95 was 9672us and enabled 7877us (ratio 0.8144127378).
These are local unoptimized native Engine measurements, not whole-application
input latency, UI build cost, arbitrary content, release-performance claims or a
universal hardware guarantee. Initial source/native initialization was explicitly
outside measurement; cold episodes still had zero composition-cache hits.

## Failure, cause and correction

The original v1 failed after 7/60 actual hits. Its 50ms free-running poll clock
admitted a background render just before the next foreground preemption. Once
that running render was canceled, its native owner was correctly retired, but
source preparation repeatedly restarted and was canceled during playback. V2's
16ms timer alone still failed after 3/60 hits. A separate Debug diagnostic run
showed repeated prepare jobs costing roughly 50ms. All failures remain archived;
the diagnostic run is not default-level performance acceptance.

The final correction arms the existing background polling path immediately after
a successful ordinary composition Present during explicit playback. This gives
optional work the full frame gap instead of a drifting timer phase. It performs
no IO, GPU work or waiting inside that Present notification. Already queued
foreground requests still win recv_timeout; existing preemption/generation checks,
CPU/GPU quotas and acknowledgment ownership are unchanged. The 16ms-only change
was withdrawn; idle polling remains 50ms. Actual native starts remain observable
at default Info, with healthy Present start/done still filtered.

Both final enabled episodes exceeded the original 30/60 minimum without changing
the test or P95 criteria. A successful Rust test alone was never considered P95
acceptance. All four episodes and raw timing/counters are retained.

## Remaining acceptance

Final-source complete workspace regression, original native blocked/idle/gap and
pressure controls, cross-platform CI, external same-path source freshness and
broader output profiles remain. The earlier workspace access violation remains
unconfirmed and is not erased by this measurement. Optional disk cache is absent;
no disk-full/corruption or physical low-memory qualification is claimed. No panel,
visual style or authored project format changed in this correction.

Archive: `data/native-prerender-paced-v3.zip`.
SHA-256: `cb1348623ca7eabc9dfc69eb727adf04d67ee8d0827d47a497cfb425a0540468`. All 46 members were read back byte for byte.
