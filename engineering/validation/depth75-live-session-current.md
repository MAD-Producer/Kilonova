# Actual retained depth Session and frozen rendering (#75)

The installed real WGSL backend now has an opt-in integration control through
the existing Session image-request, calibration, immutable snapshot and GPU
render APIs. Product code is unchanged by this test milestone. It uses the
existing captured 640x480 RGB input and pinned model/worker, with no downloads.
It does not supply fabricated qualification reports or select a device tier.

## Original setup failure

Session 39149 compiled successfully, then the new test failed: it expected one
cache miss after committing calibration, but observed two. The test setup had
incorrectly assumed that a document edit preserves the Session source revision.
`request_image_depth` intentionally binds the current engine revision; committing
the calibration advances it, requiring a new inference. The test exited 101,
the controller exited 1, and the later frozen-render steps did not execute.
Original source hashes, assertions, raw logs and consumed result are preserved.

The corrected setup adds an identical request before the document edit. All
original assertions remain: one same-revision cache miss, one hit, one context
preparation and one completed native input are checked at that point. New
assertions separately require the calibration edit to cause the second miss
and inference while preserving the same worker PID and context preparation.
No product return value, cache key, assertion, threshold or timeout was weakened.

## Actual integration outcome

Session 60556 compiled the corrected source and passed its one explicitly
selected real-hardware test: 1 passed, 0 failed/ignored and 2 filtered out, in
28.06 seconds. The controller completed in 36.546 seconds with exit 0. Source
and worker bytes remained unchanged through the consumed original command.
The worker SHA-256 was
`033b161c1789a09011162630a24430e79271b27bcddc3bb64f39986bbdef1158`.

- PNG encoding/decoding preserves the original complete RGB bytes
- The actual Session installs depth whose complete Float32 output matches
  `edb04b097a7e16b1589ce19ae7455700a357978ac01381ffbd9df703300d04af`
- An identical request hits the persistent cache without another inference
- Authoring calibration invalidates the old admission; snapshot capture refuses
  the pending state, then a fresh request establishes the revised binding
- The same native process prepares one context and completes two actual inputs
- After Session destruction, its retained worker is reaped and workspace removed
- The serialized/deserialized immutable snapshot independently verifies sources
  and attaches scalar depth to the real renderer at frames 0, 1 and 30
- All three rendered frames are identical; 627 pixels show the metric control
  plane, and 717 pixels differ from the independently rendered flat-depth control
- Modifying the owned PNG preserves the original `source_id=75` / prepared-RGB
  fingerprint error; restoring the original source permits verification again

The calibration distances (8 and 4 metres) are authored test inputs, not measured
scene distances or model accuracy claims. The control exercises visible metric
composition and frozen ownership, not independent-reference quality, automatic
tier eligibility, animation recovery, or a qualified Session binding. Images,
scalar snapshots and original source pixels remain local, outside public evidence.

## Evidence and remaining work

The [manifest](data/depth75-live-session-v1.json) and
[archive](data/depth75-live-session-v1.zip) retain both original executions and
their authored sources, commands, receipts and logs: 24 members, 39,192 bytes,
SHA-256 `9b27207899954713256e08e69b72c197c736f1e46470b1675e05165c127a3321`.
Every member passed CRC, length and SHA-256 readback. No model, RGB, depth/cache
payload, generated image, native binary or installed runtime is redistributed.

This closes the explicit unqualified backend-to-Session-to-frozen-render plumbing
control. Qualified Session installation/export remains pending, together with
native access-violation diagnosis, concurrent whole-host/device memory admission,
sustained retained-mode latency/thermal qualification, accepted cross-quality
video alignment/recovery and current-source complete regression/CI. Previous
broad failures and rejected quality/thermal qualifications remain unchanged.
No Android, panel, UI or dependency changes are included. #75 remains incomplete.
