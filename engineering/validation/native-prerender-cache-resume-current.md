# Native prerender cancellation and cache resumption (#82)

Accepted component source: `b129002440296d4e3ffd30638088b4f4c42c8e89`. Android is excluded. Engine
runtime ownership, invalidation, end-to-end native-window production and paired
foreground P95 remain pending. The prerender capability remains disabled.

Cooperative cancellation still owns its worker slot and destination until actual
acknowledgment. A completed native cache is now retained while the background
source is unready. Lookup remains closed until another preparation acknowledges
the complete frozen context, dimensions, precision and frame count. Matching
preparation reuses the same cache; changed content or output replaces it. Explicit
invalidation and disable still clear retained frames. Failed or canceled prepare
cannot reopen lookup. Retained entries remain under the existing bounded quota;
retaining a cache does not authorize restarting a failed worker.

The actual GPU control installs red pixels, publishes a second completed result,
then cancels before foreground acknowledgment. It proves the second reservation
is released, lookup closes, and same-source preparation restores the exact first
Arc and pixels without another install. A green source with identical generation
tokens and dimensions produces a different content digest and removes the old
cache. Existing assertions were unchanged.

## Evidence

- coordinator: 5 passed, zero failures, 0 excluded
- media-regression: 92 passed, zero failures, 2 excluded
- desktop-check: passed
- provenance: passed

Focused counts overlap the full media regression. Exclusions are individually
listed in the receipt and are not new acceptance. All four native precisions and
the Session window consumer remain independently accepted at earlier exact-source
checkpoints; they were not repeated for this coordinator-only change. No P95,
disk-cache failure handling or complete #82 delivery is claimed.

Archive: `data/native-prerender-cache-resume-v1.zip`.
SHA-256: `e6618aebad5fb236b967ad901666e05844c31754ffcf7045d3fc3fcba2c0e927`. All 11 members were read back byte for byte.
