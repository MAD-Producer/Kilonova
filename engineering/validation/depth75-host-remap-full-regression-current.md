# Complete workspace after retained Rust-remap integration

Issue #75 remains incomplete. The original full command at source
`dc17b0071e79e5485882c8cce5f0be2fe8e50757` failed. Its baseline was freshly
fetched main `ac65d279c57626d7aa7e28061a1a7fdf7889c68a`. All tracked source hashes
and HEAD remained unchanged until the original terminal result was consumed.

## Original command and target accounting

```text
cargo test --workspace --exclude motion-android --locked --offline --features motion-desktop/ffmpeg,motion-host/ai-depth,motion-desktop-media/ai-depth,motion-host/gpu-tests,motion-render/gpu-tests,motion-ui/gpu-tests,motion-desktop/gpu-tests,motion-desktop-media/gpu-tests,motion-graph/gpu-tests --no-fail-fast -- --test-threads=1
```

Actual Cargo metadata identified 452 expected native/doc-test invocations. All
452 were observed and independently bound to their source targets; none was
missing or substituted. There were 449 normal summaries because three native
integration targets exited abnormally. Normal summaries contain 5,290 passed,
one failed, 84 ignored and zero filtered tests. The legacy receipt field
`original_ignored` counts all ignored tests, including the new opt-in learned
host-remap controls; it does not imply all 84 predated this change. Partial
success output from a crashed target is not counted as a completed summary.

Cargo exited101. Original controller session82855 returned1 and was consumed
before source editing resumed. The controller recorded07:06:38–08:42:47 UTC on
2026-11-09; this includes preparation/accounting and is not an inference latency.
Android was excluded at the user's direction, not represented as tested.

## Failures retained without retry

- The desktop sampled-playback parent failed with its original child status
  `0xc0000005` and `evidence=Ok(())`. This is an observed child access violation,
  not an observed frame-progress assertion or a native fault location. The
  optional child-artifact environment variable was not set, so that evidence
  return does not establish retention of optional child files. Parent-captured
  stdout/stderr remain in the full log.
- `motion-graph::editable_data` exited `0xc0000005`. Its first three CPU tests
  reported success before the native uniform/byte-texture GPU control began;
  no normal target summary exists. The progress line is not a fault stack.
- `motion-render::layer_rasters` exited `0xc0000005` without a normal summary.
- `motion-render::magnified_effect_density` exited `0xc0000005` without a normal
  summary.

The desktop target separately reported1,779 passed, one failed and72 ignored.
An interim tail observation had located only the sampled child and graph
exception. Final accounting uses the complete stderr, including both additional
render target exits. Missing Epic overlay JSON loader messages do not establish
fault causation. No original assertion, deadline, ignore, backend or stack size
was changed, and no failure was rerun to obtain green output. No foreign process
or injected module was inspected or modified during this command.

## Evidence and next boundary

The [archive](data/depth75-host-remap-workspace-v1.zip) and
[manifest](data/depth75-host-remap-workspace-v1.json) retain17 verified members:
the original controller, metadata, complete raw output, final source-bound
receipt, consumed-session record, explicitly interim desktop assessment,
collection/consumption controllers and selected exact source snapshots. Every
member passed CRC, byte-length and SHA-256 readback. Archive size551,807 bytes;
SHA-256 `bddadc9cf00d67f71aba6d99b15e77dc41bd9462244150186ff1cd6f49aa0366`.
No model, runtime, executable, photograph or learned tensor is redistributed.
Selected snapshots plus tracked hashes are not a complete source archive.

The actual [32-output comparison](depth75-rust-remap-current.md) and
[cache/Session controls](depth75-host-remap-integration-current.md) remain valid
within their scopes and cannot replace this failed full command. The original
prior-source [CI](depth75-ci-53c3280d-current.md) also cannot erase these local
exits or qualify later source. The independently selected one-service sustained
experiment may proceed only after this original handle is consumed; it is not
a retry of these failed targets or a full-workspace acceptance pass.
