# Engine prerender native cancellation and observation controls (#82)

Accepted component source: `9e1eea7fef55fa9d2907739f91dc56b98cb2f64f`. Android is excluded. The
experimental profile remains idle-only High, authored camera, single-time output.
General prerender capability stays false. This checkpoint does not complete #82.

The controlled native backend blocks inside the actual background decoder. Engine
State still replies. A second gate then blocks the actual serial Engine handler;
the independent observation API still returns the same native render ticket and
video phase without enqueuing State. Foreground requests queue while that handler
is blocked. Dispatch cancellation already reaches the published native control.
The decoder is released while the serial gate remains blocked. The independent
watch must report completion waiting with zero retained CPU bytes, and the serial
in-flight operation must still be test_wait. A successfully published frame would
retain its CPU permit, so a later disable handler cannot mask missing dispatch
cancellation. Only then is the serial gate released. This leads to zero
installed completions, no fault promotion, disabled policy, native owner stopped
and zero retained CPU bytes. The tiny owned video/index and controlled decoder
are deliberately not real FFmpeg decode or GPU performance evidence.

The actual native-window cold/warm/edit pixel control reran with unchanged
assertions. Engine State now exposes Session.cached_presented separately from
lookup hits. Both APIs report one successfully committed cache presentation;
editing red to green uses ordinary rendering and leaves that count unchanged.
Pixels are read from the successfully committed retained front, not the physical
display. Native destination installation logs and flushes its start with ticket,
frame and document/device generation before entering GPU upload.

## Evidence

- blocked-runtime: 1 passed, zero failures or exclusions
- publication-gate: 2 passed, zero failures or exclusions
- native-engine-owner: 1 passed, zero failures or exclusions
- queue-regression: 4 passed, zero failures or exclusions
- diagnostics-regression: 12 passed, zero failures or exclusions
- desktop-check: passed
- provenance: passed

These 20 selected tests passed at this exact source. Full Host/Render/media,
all-four-precision native copies and Session window controls remain separately
bound to their historical checkpoints; they are not full regression of this
rebased Engine source. No existing assertions were weakened. The prior starvation
failure and successful deadline fix are retained in the separate Engine owner
archive and completion document.

Archive: `data/native-prerender-engine-controls-v2.zip`.
SHA-256: `3d246396267353f6a5c4daefac8b947882119e3b06632130342cc54a0ee04f32`. All 25 members were read back byte for byte.

Remaining: explicit playback demand, externally changed same-path sources,
broader preview profiles, pressure/invalidation/reopen controls, paired foreground
P95, fresh-main combined full regression and cross-platform CI. Optional disk
cache is absent; no disk-full or corrupted-cache proof is claimed. Managed revision
and explicit refresh are the current source invalidation contract.
