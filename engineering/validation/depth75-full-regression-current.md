# Original complete maintained-workspace regression

Issue: #75. Source `ad799f2f634b5a3897c7df2e06053216db9e86a4` remained unchanged
through the original command and its consumed terminal session. Its main
baseline was `ac65d279c57626d7aa7e28061a1a7fdf7889c68a`. This is a record of a
**failed** original acceptance command, not a waiver or replacement narrow test.

## Command and accounting

```text
cargo test --workspace --exclude motion-android --features motion-desktop/ffmpeg,motion-host/ai-depth,motion-desktop-media/ai-depth --no-fail-fast --locked --offline -- --test-threads=1
```

The actual Cargo metadata identified 449 expected test/doc-test invocations.
All 449 were observed; no expected source target was missing and no unexpected
source target was substituted. Native executable ownership was checked against
its actual dependency-info file. There were 447 normal target summaries because
two integration targets exited abnormally.

Completed summaries contain 5,274 passed, two failed, 78 original ignored and
zero filtered tests. Successes printed before an abnormal target exit are not
invented as completed-summary results. Cargo exited 101. The observer returned
1 after retaining this failure; original session 87729 was consumed. Elapsed
time was 5,374.734 seconds. Android was excluded at the user's direction; it is
not represented as tested.

## Original failures

- `engine::failure::composition_tests::accumulator_admission::resized_accumulator_refusal_preserves_pixels_cause_and_default_output_channels`:
  the parent asserted that its child succeeded. The original test does not print
  that child's exit code; an exact native exception or pixel mismatch is not
  inferred.
- `engine::sampling_playback_tests::paced_sampling_playback_records_native_costs_without_starving_frames`:
  the parent retained `child=exit code: 0xc0000005 evidence=Ok(())`. This is an
  observed child access violation, not an observed playback-frame assertion.
  The optional sampling-artifact environment variable was not set; the evidence
  status does not prove optional child artifacts were retained.
- The `motion-effects` `packages` target and `motion-render` `pointwise_fusion`
  target each exited `0xc0000005, STATUS_ACCESS_VIOLATION`. Their partial output
  and missing normal summaries remain in the final accounting.

The desktop main target separately reported 1,778 passed, two failed and 72
original ignored tests. Coincident missing Epic overlay JSON loader messages do
not establish fault causation. There was no debugger/module inspection, foreign
process termination, backend/stack/deadline alteration, failure retry or change
to original assertions. Later successful remote CI does not erase these exits.

## Evidence

The [manifest](data/depth75-full-regression-v1.json) and
[archive](data/depth75-full-regression-v1.zip) retain the actual controller,
command, Cargo metadata, complete stdout/stderr, final source-bound receipt,
consumed-session record, interim failure assessment and selected source files.
The archive has 15 members, 526,767 bytes, SHA-256
`d67ccc627bcf58a842fed1b0758eae7657f7de8afdb5a2177f8c4b8fe7cc98ba`.
Every member passed CRC, byte-length and SHA-256 readback. Selected source
snapshots and complete tracked-source hashes do not constitute a complete
repository archive. No models, weights, photographs or learned arrays are
distributed.

The [current-source remote checks](depth75-platform-memory-current.md) and
[actual sustained/alignment observations](depth75-sustained-alignment-current.md)
retain their separate evidence and limits. The explicitly selected finite
retained-context experiment may proceed only after this original handle is
consumed; it is neither a retry of these failures nor whole-workspace acceptance.
PR #651 and issue #75 remain open pending the actual remaining requirements.
