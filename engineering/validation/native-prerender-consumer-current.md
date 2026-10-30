# Native foreground cache consumer (#82)

Status: Session consumer accepted at `233463fc5310abaa40bbfed26ec9e4482a46a733`. Engine scheduling,
foreground P95 and combined desktop/cross-platform acceptance remain pending.
The prerender capability remains disabled. Android is excluded. No panel,
layout, style, dependency or asset changes are included.

The owner offers an exact context and frame. Session additionally checks its
document/surface generations, canonical working format, authored dimensions,
High mode, active camera and memory quality level zero. Ineligible entries use
the unchanged scene-rendering path. Same-device copy validates both resource
owners, encoder owner, dimensions, format and COPY_DST usage before encoding.
Only preview native targets gain COPY_DST; capture targets retain their existing
usage contract. Four-precision readback verifies exact encoded UNORM storage,
floating HDR/negative channels and alpha after transfer to the preview candidate.

The existing SDR output conversion, surface acquisition, overlay, allocated
submission, health check and retained-front commit remain the publication path.
The saturating lifetime cached_presented count increments only after successful
submission/commit/present; it is separate from lookup hits. Cached transfers do
not resolve unwritten composition timestamp slots. The worker flushes its Info
start before entering the handler. Actual default logger blocking/sink proof
is still pending; the GPU controls alone do not establish it.

The real-window control uses green cache pixels and a red live scene. It verifies
successfully committed retained-front pixels, one overlay call, stale context/frame,
Balanced/observer fallbacks, foreign owner rejection, and unchanged prior front
after refusal. All four precisions run explicitly in an isolated child process.
Exposure, Auto/observer/reduced-profile caching and Engine-driven native-window
production are not claimed by this checkpoint.

## Actual evidence

- desktop-check: passed
- native-copy: 9 passed, zero failures, 2 excluded
- native-copy-four-precision: 1 passed, zero failures, 0 excluded
- window-consumer-four-precision: 1 passed, zero failures, 0 excluded
- host-regression: 263 passed, zero failures, 1 excluded
- render-regression: 876 passed, zero failures, 3 excluded
- worker: 6 passed, zero failures, 0 excluded
- provenance: passed

Focused counts overlap the full regressions and must not be added as unique tests.
Manual four-precision upload/copy and real-window controls ran explicitly with
zero exclusions. Other excluded controls are listed individually in the receipt;
their exclusion is not acceptance. The v1 compile failure from inconsistent
scene/copy completion types is preserved; the implementation was corrected
without changing existing assertions.

Archive: `data/native-prerender-consumer-v1.zip`.
SHA-256: `2e78529b6408313715674e35e8b617d8e20090a3affc14d8d4bbcab295db6316`.
All 23 archived members were read back and verified byte for byte.
The source-bound receipt is prerender-consumer-v2/accepted.json inside the archive.
No disk cache, disk-full recovery, P95 guarantee or complete #82 delivery is claimed.
