# Paired native playback protocol (#82)

This protocol is fixed before measuring the draft. It contains no accepted native
results. The manual Rust study and the pressure/lifecycle control remain unexecuted
until the independent maintained-workspace validation owner terminates.

The ignored study uses an owned animated OBJ triangle, a 640x360 native window and
High authored-camera single-time rendering. Four fresh Engine episodes run in
disabled/enabled/enabled/disabled order. Each requests frames 1 through 60 at an
absolute 30 fps cadence. The initial presentation warms common source/shader work;
the disabled episodes have no composition-frame cache. Enabled episodes prewarm a
three-frame window, then must actually consume at least 30/60 cached presentations.
Warmup alone never contributes a hit. Final retained-front pixels must match all
four episodes exactly, show the model, and leave the authored project unchanged.

Each raw frame records Seek plus successful Present foreground cost, their separate
costs, Present polls, deadline misses, actual cachedPresented and bounded runtime
status. State collection and final readback are outside the timed interval. No
per-frame PNG capture is performed. A failed child preserves partial raw samples,
default-level file logs, stdout and stderr; they are diagnostic evidence, not a pass.

Nearest-rank P95 is computed from each episode's 60 raw costs. Pair episode 1 with
0 and episode 2 with 3. Both enabled/disabled P95 ratios must be at most 1.0, and
the pooled enabled P95 must be no greater than pooled disabled P95. A green Rust
test alone cannot establish this. Malformed, incomplete or inconsistent reports
are rejected; a valid measurement showing regression exits with status 1 and
retains the actual regression. Synthetic parser tests are not performance evidence.

Reproduce after loading the repository's native FFmpeg build environment:

```powershell
$env:MOTION_PRERENDER_PACED_ARTIFACTS = '<fresh absolute evidence directory>'
cargo test -p motion-desktop --features ffmpeg,motion-host/ai-depth --locked --offline --bin motion-studio engine::prerender::tests::performance::paced_model_playback_records_paired_foreground_costs_and_real_cache_hits -- --ignored --exact --test-threads=1
python tools/analyze_prerender_paced.py "$env:MOTION_PRERENDER_PACED_ARTIFACTS/paced-costs.json" --output "$env:MOTION_PRERENDER_PACED_ARTIFACTS/assessment.json"
```

Run one own Cargo owner at a time. Keep source hashes, exact command and compiler
profile with the measurement. This Windows Engine-only study excludes UI build,
whole-app input latency, arbitrary source replacement, other preview profiles and
physical low-memory-device qualification. Concurrent machine activity and hardware
affect timing; a local pass is not a universal performance guarantee. Android is
retired and outside the maintained scope. These controls do not close the issue.
