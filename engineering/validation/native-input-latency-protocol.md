# Native input latency study for background prerendering

Status: implementation prepared; native execution and P95 acceptance pending.
This document is a protocol, not a completion receipt. It does not close #82.

The earlier Engine-only playback study does not measure the application's input
path. This separate study uses a real native window, production event translation,
panel hit testing, action dispatch, worker acknowledgments, UI construction and
successful native presentation. It injects `CursorMoved` and `MouseInput` events;
it does not measure OS device delivery or scheduler latency.

## Frozen contract

- Use fresh Shell/Engine instances in disabled/enabled/enabled/disabled order for
  each of two scenarios: property dragging and timeline scrubbing
- Use 60 complete press/move/release samples per episode, a 640 by 360 owned
  composition, a 1280 by 800 native window and High authored single-time output
- Configure the policy through the production tool router outside measurement
  and reset its generation before each sample; enabled samples must actually
  warm native frames and retain an independent worker observation
- Begin timing before event translation; include every control pass, command
  acknowledgment, UI build and successful native Present; stop before artifact
  serialization, state diagnostics, pixel readback or undo/redo verification
- Retain partial samples on failure and all measured costs; no outlier removal,
  best-run selection, threshold changes or unchanged retries to obtain green
- Verify the actual sampled property/frame and all full-resolution pixel hashes
  against the disabled reference episode; verify one undo entry per property
  gesture and no project mutation from timeline seeking
- Retain worker request IDs, queue/work timings, independent native observations,
  adapter identity, revision, policy state and action counts; worker diagnostics
  collected after a state barrier can also include background work outside the
  measured interval and must not be blindly summed as an input latency
- Independently calculate nearest-rank P95 from every raw sample; require both
  enabled/disabled pairs and pooled enabled/disabled P95 to be at most 1.0

The heavy study is explicitly ignored in ordinary unit tests. Run the exact test
with `--ignored --exact --test-threads=1`, an owned
`MOTION_PRERENDER_INPUT_ARTIFACTS` directory and default `MOTION_LOG`:

```text
engine::prerender::tests::input_latency::native_controls_record_edit_and_scrub_latency_with_real_optional_work
```

Then run `python tools/analyze_prerender_input.py PATH/input-costs.json`.
The parser's synthetic controls exercise malformed data and rejection logic;
their success is not native performance evidence. Test process success alone is
also insufficient: preserve its exit status and the independent P95 assessment.

This fixture changes no production panels, control behavior, layout or styles.
Android remains outside the maintained desktop acceptance scope. Source freshness,
full workspace regression and cross-platform CI remain separate gates.
