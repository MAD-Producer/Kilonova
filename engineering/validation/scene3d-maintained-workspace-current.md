# Maintained workspace and four-platform checkpoint

The complete maintained workspace passed against source head
`594e810df818bdafe34f0b9c08455828168b634a`. Its complete Git tree matches the
Chinese squash merge `ed1a84a4d22465601564c761ebdf3a8b01db88de` and the main
baseline used by the following owned-model import increment.

This checkpoint covers the merged 3D/depth/aperture source. It does not cover
the later desktop model-import or approved model-library interaction changes;
their source-bound controls and published-head CI are separate gates.

## Local execution

The actual full command was:

```text
cargo test --workspace --exclude motion-android --locked --offline --features motion-render/gpu-tests,motion-graph/gpu-tests,motion-host/diagnostics,motion-host/ai-depth,motion-host/gpu-tests,motion-ui/gpu-tests,motion-desktop/gpu-tests,motion-desktop/ffmpeg,motion-desktop-media/gpu-tests,motion-desktop-media/ai-depth -- --test-threads=1
```

It produced 3,779 passed, zero failed and 30 existing ignored manual controls.
Three of those manual controls were then executed explicitly without modifying
their product source or assertions: history sampling, keyframe timing and image
preview resampling. Each passed. The distinct accepted total for this checkpoint
is therefore 3,782; 27 other manual, hardware, performance or screenshot fixtures
remain enumerated and are not counted as passes. Android is retired and excluded.

Desktop check with FFmpeg/AI-depth, style-consumer checks and provenance validation
also passed. The controller compared source/configuration hashes throughout.

Two unsuccessful setup attempts are retained in the evidence:

- The first manual-history command supplied a workspace-only feature selection
  to the core-only package and exited before running that test. The corrected
  command ran the unchanged test successfully.
- The first manual-image input was 1024x1024, already smaller than the existing
  2048 preview bound. The unchanged test correctly did not observe a resampled
  cache hit. A separately generated procedural 3072x2048 checker input exercised
  the intended reduction and passed. This is a resampling control, not proof of
  photographic quality or a user asset acceptance.

The complete workspace was not rerun merely to hide either setup failure. Raw
outputs, exact commands, source hashes, corrected controls and exclusions are
retained together.

## Cross-platform execution

Own PR #558 passed all five required exact-head checks before its guarded squash
merge. The resulting main run
[40857490907](https://github.com/MAD-Producer/Kilonova/actions/runs/40857490907)
subsequently completed successfully: dependency audit, Rust/GPU, libav, Windows,
Linux, macOS arm64, macOS Intel and desktop release publication all succeeded.
The exact head and per-job results are retained in the archive. Later PRs must
pass their own required checks; this run cannot stand in for a changed head.

## Evidence

[Verified raw checkpoint archive](data/scene3d-maintained-workspace-v1.zip),
SHA-256 `02046fb67ff3ef436eacb36b6594522d72c7ca9802f29ba381837f79e4a97fc2`.
All 34 entries were read back and checked. The archive contains the actual raw
test outputs, controller sources, receipts and source/configuration digests;
it excludes compiled binaries and third-party original sample assets.

This checkpoint does not enable background prerendering. The separate #82
scheduler still requires actual render/cache integration and runtime measurements.
