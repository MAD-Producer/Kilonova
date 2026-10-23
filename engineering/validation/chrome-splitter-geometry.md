# Reuse splitter geometry within one chrome frame

Related #134; the broader hot-path issue remains open. Baseline main is
`10334cb3`. Before starting, the audit checked 363 head/remote references,
36 distinct chrome sources, and 106 worktrees. No dirty splitter changes were
found. The only historical single-call candidate (`5f7e5d95`, October 11)
predates the present base-fill/cursor and animated overlay phases; it is not a
geometry-sharing implementation for the current renderer.

Current chrome collects splitter handles twice for the same immutable dock tree
and bounds in one call. The first phase paints their base and establishes resize
cursors. The later phase paints animated accents in its existing overlay order.
This change shares the frame-local result between those phases. Their paint
positions, motion IDs, handle order, hit regions, menu guards, layout bounds, and
workspace drag/persistence ownership remain the same.

It does not merge the two paint phases: moving one through menus/overlays could
change z-order. It introduces no cross-frame cache, new invalidation key,
dependency, asset, logger, stdout output, error structure, or worker behavior.
This is a performance correction; existing errors, cancellation, and disabled
branches retain their original boundaries. No new observation is claimed.

## Experiment and verification

Both the original and final real-chrome probes passed (each 1 test, 0 failed,
0 ignored). They are before/after performance experiments, not evidence of a
functional failure in the original paint output. The intermediate borrowed
second-pass experiment is retained separately and is not added to final counts.

`after-final/comparison-receipt.json` verifies every one of the 32 cases:

- SHA-256 of the full shape, color, glyph, and operation-order dump is identical
  before/after; resize cursors and draw counts also match.
- The measured reduction equals exactly one geometry collection per frame:
  default layout (seven handles) saves 2 allocation requests / 432 requested
  bytes per frame; alternate layout (three handles) saves 1 / 144 bytes.
- Across 32 × 20 measured frames, that is 960 requests and 184,320 requested
  bytes removed. Those are totals over 640 frames, not a per-frame result.
- No project actions or undo entries are generated in the probe cases.

After removing instrumentation, 13 existing desktop chrome tests and 6 existing
`dock_constraints` tests passed, with 0 failed and 0 ignored. They cover main/
floating input and modal ownership, small-window geometry, actual drag limits
for all four edges, nested handle hit boundaries, and saved split ratios.
These are 19 existing regressions, not 19 new tests. Product `cargo check` passed
with 19 existing warnings. Logs are `final-chrome-tests.log`, `dock-tests.log`,
and `check.log`. Provenance, panel style-consumer, and diff checks passed.
The final source was also compared against the baseline transformed by only
the planned collection reuse. Exact-head CI and merge remain pending.

The local probe calls actual `chrome` with a real core demo-project snapshot,
system font atlas, default dock layout and an alternate nested layout covering
all four splitter edge directions. Two locales, two sizes, two layouts, and
four pointer/modal states produce 32 cases. A thread-local allocator measures
20 warmed frames per case and separately measures one geometry collection.
It counts successful allocation/reallocation requests and requested bytes,
not peak live memory or GPU memory. A String allocation is the canary.

The probe seeds settled divider values through the existing `Motion::value_at`
API so full paint order can be compared without wall-clock animation drift.
Shapes, colors, glyphs, paint-operation ordering, and resize cursors are recorded.
No project actions or undo entries are expected. These are actual UI draw/input
paths, not physical OS window, full animation timing, DPI, or GPU pixel proof.

Probe source and results stay under `.tools/chrome-splitter-geometry` in the
shared workspace. Temporary absolute test-module includes and the allocator
were removed after the final comparison; the product patch does not install them.

The shared vector is consumed in the second phase so it is freed at the end of
that phase, as the old second collection was. No peak-live-memory, total-frame
allocation elimination, whole-workspace geometry cache, or FPS gain is claimed.

| Audit ID | Classification and evidence |
|---|---|
| SG-PERF | New correction: one frame-local collection replaces two identical collections; exact measured deltas verified in all 32 cases. |
| SG-PAINT | Existing behavior retained: complete paint commands and ordering have equal hashes in all tested states. |
| SG-INPUT | Existing boundary retained: cursor/menu checks pass; Workspace still owns resize interaction and persistence. |
| SG-NORMAL | Idle, disabled/modal, and cancellation remain normal; no new fault handling. |
| SG-OBSERVE | No new observation or log site; existing errors/return semantics are untouched. |
| SG-PENDING | CI/merge, physical windows/DPI, animation timing, and broader #134 work remain unproven here. |

## DESIGN and provenance

PROVENANCE.md, ATTRIBUTION.md, NOTICE, and DESIGN.md were checked. No third-party
assets, copied documentation, compatibility names, dependencies, new colors,
dimensions, tokens, style names, controls, or reduced hit tolerances are added.

| DESIGN §0.7 baseline row | Disposition |
|---|---|
| Path six row steps | retained with reason: path rows untouched. |
| Path two-column split | retained with reason: columns untouched. |
| Path button/field heights | retained with reason: controls untouched. |
| Path numeric background | retained with reason: numeric editing untouched. |
| Path boolean wide button | retained with reason: boolean editing untouched. |
| Effect row heights/steps | retained with reason: effect rows untouched. |
| Effect boolean buttons | retained with reason: boolean controls untouched. |
| Effect enum wide buttons | retained with reason: enum controls untouched. |
| Effect hover alpha | retained with reason: effect hover states untouched. |
| Effect indentation/hierarchy | retained with reason: property tree untouched. |
| Timeline label coordinates | retained with reason: compatibility baseline preserved. |
| Timeline snap color | retained with reason: existing drag feedback preserved. |

DESIGN §12 acceptance is limited to the verified splitter geometry, cursor/input
behavior, paint commands, and allocation scope. All-panel visual remediation,
physical-window/DPI screenshots, FPS, and total frame-layout caching are pending.
