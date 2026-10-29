# Sustained native real-codec depth playback checkpoint

This is actual host Session playback through a native window surface, not
offline movie export and not a mocked successful render. The independently
authored source is 1,800 H.264 frames at 30 fps, 32 by 32 pixels. Every source
sample has explicit indexed-PTS calibration. A controlled scalar backend derives
depth from the actual decoded red channel; varying green/blue source markers
make a stale or wrong video/depth pairing observable in captured pixels.
The renderer uses High preview and default Unorm8. The fixture is intentionally
small and does not establish high-resolution or trained-model performance.

## Measured run

| Observation | Actual result |
| --- | --- |
| Timed wall interval | 60.010626 seconds |
| New frames presented in that interval | 1762 |
| New-frame rate for that interval | 29.361 / second |
| Render attempts | 10244 |
| Render-call P95 / maximum | 13831 / 32407 microseconds |
| Independently verified checkpoints | 62 |
| Stored depth samples | 1775 |
| Final live retained samples | 128 (limit 128) |
| Backend inferences | 1786 |
| Peak logical GPU admission | 1690840 bytes (shared limit 64 MiB) |
| Admission after Session drop and actual GPU completion | 0 bytes |

The first checkpoint is frame 0 and the final is frame 1799. At each retained
checkpoint, the depth PTS and independent original decoded-RGB fingerprint match
the displayed source. Native-front readback proves both a blue physical
occluder over far samples and original video color over near samples. Budget
admission stays within its limit; live retention stays at most 128 while actual
stored samples exceed that cache. Presented frame numbers increase without
requiring each polling call to advance a frame. Skips and pending attempts
remain visible in the raw report rather than being hidden by a timing assertion.

Source encoding, independent decode, project construction and first-pipeline
warmup occur before the timed interval. Reported P95 measures render calls,
not full input-to-display latency. Checkpoint pixels do not prove pixel identity
for every intervening frame. This run does not claim adaptive-quality, audio
playback, full desktop chrome, trained-depth-model quality, arbitrary codecs,
high-resolution throughput or sustained aperture rendering. These limits do
not turn a 60-second actual playback run into a longer-duration benchmark.

## Reproducibility and setup failures

The JSON report records the actual adapter, source/configuration hashes, exact
probe binary and tool hashes, default-level logs, caller output streams,
all presented frame numbers, checkpoint pixels and resource counters. The
owned source movie and authored project are retained. Existing validated
FFmpeg DLLs were staged beside the standalone executable with exact SHA-256
checks; the runtime manifest is retained but DLLs are not bundled here.

The first attempt failed to compile because the standalone probe used a
crate-private texture accessor. The second built but failed at Windows loader
startup before application code ran. These are retained setup failures, not
failed playback controls. The public texture-view accessor and normal staged
runtime resolved setup; no product source or expected playback assertion was
changed. The accepted run is version 3.

Archive: `data/scene3d-long-native-playback-v1.zip`. Every entry and CRC were read back and verified.
SHA-256: `9de11a2c2857874b48f381b96fbefc5f29835e2c5f6867dbe4b06b20beb49425`. Source-bound receipt:
`artifacts/pr-queue/long-native-playback-v3/accepted.json`.

Official alpha comparison and latest-main complete workspace / maintained
desktop cross-platform acceptance remain separate. Issue 87 remains open.
Background prerender issue 82 still needs actual render/cache integration.
