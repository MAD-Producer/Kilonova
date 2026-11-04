# Issue 88: current-main MCP replacement workflow

Status: focused native workflow passed on `f645ca3c36e44235ef146f8acfba52605abc7242`.
This integrates paired-publication parent `265f6668` and main `77f61452`.
Current-head CI remains required; earlier green checks are historical.

Three existing MCP unit cases passed unchanged. They exercise the strict action
contract, all binding schemas and actual asynchronous preparation plus atomic
saved publication. One actual Windows Surface test passed with no ignores:
the original retained pixels survive refusal; successful paired publication
invalidates the front; unchanged presentation reuses it; undo restores the old
pixels and redo/reopen restore the new pixels. Representative before, retained,
after and reopened captures were viewed, alongside the executable's byte/pixel
and counter assertions. This is committed-front readback after real native
presentation, not physical scanout or real photographed tracking acceptance.

The FFmpeg-enabled desktop executable built successfully. That exact binary,
SHA-256 `604d2b49fa00cdf3ac34d3d4ac6d39951cf900aa19949d66c242512b8c58cc7e`,
then passed all ten existing stdio workflow controls. A normal process returned
22 JSON-RPC replies; a separately opened read-only process returned seven.
The controls cover advertised actions, stale/unknown-field rejection,
cancel/release, asynchronous native preparation, wrong-original refusal,
one-revision stable-ID commit, undo/redo/save, reopening and read-only denial.
With MOTION_LOG unset, actual Info starts reached the file, original Error
records reached both file and stderr, and every stdout record remained valid
JSON-RPC. There were no retries or relaxed assertions.

All Rust/WGSL/Cargo source hashes stayed unchanged across compilation, tests
and real protocol processes. Complete Git subtrees for model, core, host, media
and render equal parent `265f6668`. Its scoped twenty-test evidence remains
bound to its recorded source, and is not added to this PR's new counts.
No panel differs from main. No product behavior, dependency or asset was added
as part of this integration.

The [receipt](data/tracking88-mcp-current-main-v1.json) and
[archive](data/tracking88-mcp-current-main-v1.zip) retain exact source identities,
raw command output, Surface counters and captures, actual child process exits,
JSON-RPC streams and default logs. Every member was read back and checked.
The original build session 87354 and wire session 17299 terminated successfully
and their results were consumed before publication.

This is not a fresh complete desktop/workspace run. The known earlier native
crashes remain unresolved, the point/plane prototype still fails photographed
quality acceptance, and issues 85/86/88 and the full 3D route remain incomplete.
Android is retired and outside acceptance.
