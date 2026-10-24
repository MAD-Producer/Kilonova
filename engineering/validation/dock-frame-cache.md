# Dock frame cache — #134 partial delivery

Baseline: `81aca69996a629643faebc71345c60afe054a6e1` (main, #466).
Branch: `codex/dock-frame-cache`. This implements only the `(layout, size)`
frame-result cache item in #134; the aggregate issue stays open.

## Change and boundaries

`panels::frames` previously called `dock::solve` and rebuilt frames on every
query. It now retains one entry per calling thread. The key records every Node
variant/field, child and tab order, tab-list length, active index, dock ID,
edge, split/fixed-size float bits, and both window-size float bits. Matching
uses the borrowed tree without allocating a hash/string/serialized key.
Exhaustive key-writing patterns make newly added Node fields a compile error.

The original calculation body is retained verbatim as `calculate_frames`.
The API still returns an independently owned `Vec<Frame>`; mutating a returned
vector cannot change future queries. `Frame` is now Copy (only its existing
panel ID and two rectangles). No caller, docking state, serialization, command,
history, styling value, or hit tolerance is changed.

Both retained buffers have a 128-item cap and are reused across key changes;
128 is a metadata/result count, not a UI dimension. Oversized trees/tab lists,
oversized results, and nonfinite geometry inputs decline admission and keep the
original result. Signed zero is distinguished. Replacing an entry evicts the
previous key. A busy RefCell or destroyed TLS slot also uses the original
calculation. Calculation occurs outside the cache borrow. These optional-cache
misses are normal paths, not errors, and add no hot-path logging or file IO.

## Evidence

Before implementation, an audit inspected 372 refs, 41 distinct panel-source
versions, 106 worktrees, and open PRs. No overlapping frame-cache work was found.
Other toolbar work (#467) was separate and has since merged. Audit artifacts
are under the shared workspace `.tools/dock-frame-cache` directory.

Temporary independent instrumentation called actual `chrome` with a real core
demo snapshot and the existing system-font atlas. English/Chinese, 900×600 /
1600×1000, default / alternate all-four-edge layouts, and neutral / hover /
drag / modal states form 32 cases. Settled splitter animation values were
seeded through the existing Motion API to avoid wall-clock drift. Each case
has three cold/warm-up draws followed by 20 measured draws. A different sentinel
layout forces a cold cache before each case. Allocator instrumentation counts
successful allocation/reallocation requests and requested bytes, with a String
allocation canary; it does not measure peak live memory, GPU use, or FPS.

All 32 complete paint-command hashes, frame rectangle coordinates, shape/glyph
counts, ordering, and cursor records match the uncached baseline. Each case
changes from repeated cold solves to one cold solve and zero warmed solves.
For the default English 900×600 neutral sample, the three initial draws go
from 12 solves to 1; the following 20 draws go from 80 solves to 0. Those 20
draws go from 9,240 allocation requests / 12,691,520 requested bytes to
9,000 / 12,660,800. These are sample totals, not per-frame or whole-application
measurements. Baseline and final probes each pass their single experiment;
this is a measured optimization, not a claimed old functional test failure.

Probe sources, original files, before/final logs and the 32-case comparison
receipt remain locally available under `.tools/dock-frame-cache`. Temporary
allocator/module includes and the solve counter were removed from product code.
Each nonempty owned-result hit still allocates one result vector and compares
the bounded structural key. Large layouts remain uncached. No total frame
allocation elimination or universal one-solve-per-frame claim is made.

Permanent verification comprises four actual-frame unit tests and four isolated
cache/allocation tests. The frame tests compare the original calculation across
tabs, active/fallback/unknown panels, sizes and nonfinite inputs; mutate returned
vectors and layouts; hold the actual cache borrow; and destroy the actual cache
before a later TLS destructor requests geometry. The allocator tests check 120
nonempty hits require exactly 120 vector allocations, 32 changed keys reuse both
buffers with zero store allocations, all key fields/float bits invalidate, and
capacity/nonfinite declines retain the previous valid entry.

After removal of temporary instrumentation, all 30 permanent regressions pass:
4 new actual-frame tests, 4 new isolated cache/allocation tests, 13 existing
chrome tests, 3 existing workspace maximize/restore tests, and 6 existing dock
constraint tests, with 0 failed and 0 ignored. No assertions are weakened.
Logs are `unit-tests.log`, `standalone-tests.log`, `chrome-tests.log`,
`maximize-tests.log`, and `dock-tests.log` under the local evidence directory.
Source verification confirms the raw calculation body is verbatim, main.rs is
unchanged, and temporary probe hooks are absent. New-file rustfmt, provenance,
panel-style-consumer and diff guards pass. Product `cargo check` passes with
19 existing desktop warnings; its output is `check.log`.
Exact-head five-gate CI and merge remain pending.

| Audit ID | Disposition and limits |
|---|---|
| FC-PERF | New optimization: actual 32-case solves and allocation requests reduced; requested bytes are not peak memory. |
| FC-KEY | New cache boundary: complete typed key, bounded buffers, replacement and returned-value independence covered. |
| FC-GEOMETRY | Existing calculation retained; actual frame and full-chrome paint comparisons match. |
| FC-NORMAL | Normal miss, oversized/nonfinite admission decline, busy borrow and actual TLS destruction preserve calculation. |
| FC-OBSERVE | No new observation, Error/Err/panic swallowing, worker handler, timeout, retry or protocol boundary. Existing failures are unchanged. |
| FC-PENDING | Native physical windows/DPI, live animation timing, GPU pixels, FPS, CI/merge and remaining #134 work are not established by these probes. |

## DESIGN and provenance

PROVENANCE.md, ATTRIBUTION.md, NOTICE, and DESIGN.md were checked. Implementation
and probes are independently authored. No dependency, third-party asset,
compatibility name, copied UI detail, color, dimension, token or style is added.

| DESIGN §0.7 baseline location / before | After and evidence / disposition |
|---|---|
| path_controls.rs six literal row steps | retained with reason: file/row layout untouched; no new screenshot claim. |
| path_controls.rs two-column split | retained with reason: columns untouched. |
| path_controls.rs mixed button/field heights | retained with reason: controls and hit areas untouched. |
| path_controls.rs numeric background | retained with reason: field rendering untouched. |
| path_controls.rs wide boolean button | retained with reason: boolean controls untouched. |
| effect_controls.rs row heights/steps | retained with reason: row rendering untouched. |
| effect_controls.rs boolean buttons | retained with reason: boolean controls untouched. |
| effect_controls.rs full-width enum button | retained with reason: enum controls untouched. |
| effect_controls.rs hover alpha | retained with reason: hover styles untouched. |
| effect_controls.rs indentation/hierarchy | retained with reason: property tree untouched. |
| timeline.rs label coordinates | retained with reason: compatibility coordinates preserved. |
| timeline.rs snap-guide color | retained with reason: existing drag feedback preserved. |

DESIGN §12 acceptance is limited to the measured geometry, full paint-command
comparison and existing input/layout regressions. Compilation is not visual
acceptance. All-panel remediation, physical Windows/DPI screenshots, every
workspace/native ownership transition and Info-panel/FPS comparisons remain
outside this delivery's evidence. Cache unit tests do not replace those checks.
