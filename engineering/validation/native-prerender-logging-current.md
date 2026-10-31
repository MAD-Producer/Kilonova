# Native prerender default logger controls (#82)

Accepted source: `d1aa533134b4e5c47c80cc0a57c63e503afc08cb`. Android is excluded. Engine ownership,
runtime invalidation, end-to-end cached presentation and foreground P95 remain
pending. The prerender capability remains disabled.

The worker flushes its Info start before entering the handler. A process-isolated
buffered file sink blocks at the actual worker start hook and reads the file
without a test-side flush. The start must already be on disk, and the prepare
handler must not have begun. This prevents a later decoder flush from masking
a missing worker flush. This guard passed once explicitly and also within the
seven-test worker suite; those counts overlap.

A separate process-isolated test uses the actual desktop logger with MOTION_LOG
unset. The actual native worker enters the controlled video backend and blocks.
The test reads the render start from motion-studio.log before releasing decode,
without flushing the sink itself. The original controlled decode failure then
appears in both the file and stderr and is absent from stdout. Worker phase,
ticket, acknowledgment and CPU permit release are also checked. This test passed
with zero failures and exclusions. Its tiny owned video/index fixture and
controlled backend do not establish real FFmpeg decoding or performance.

Archive: `data/native-prerender-logging-v1.zip`.
SHA-256: `4b21a6ac09a44bb0c0e875c90b82f3ff6525e390dc31d775a3a20c20c0159ea2`. All 16 members were read back byte for byte.
The archive includes the source-bound receipt, commands, both captured streams,
blocked file snapshots and the final actual desktop log. Full Host and Render
regressions remain separately bound to the consumer checkpoint; they were not
repeated for these test-only additions. Existing assertions were unchanged.
