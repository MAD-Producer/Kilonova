# Issue #262: lossless XYZ merge API

Scope: backend portion of [#262](https://github.com/MAD-Producer/Kilonova/issues/262), also contributing to the shared API coverage in #86. Branch `feat/262-merge-dimensions` starts at GitHub `main` commit `1750d8dc`. The previous workspace-performance work remains in its existing worktree. GitHub was read on 2026-10-17: 123 open issues and no open pull requests; #262 had no comments or assignees.

## Implemented contract

`{"op":"merge_dimensions","composition":"comp-main","object":1,"property":"position"}` uses the existing command parser, registry, revision checks, composition routing, atomic batch, history and persistence paths. It supports layer position/rotation/scale and the same camera position/target restrictions as separation. It does not add a user-visible control.

The first version intentionally accepts a conservative, provable subset: the axes have identical key counts and times, and each actual outgoing segment has identical effective interpolation. All-static axes are supported. A shared custom curve overrides otherwise differing ease values; the last key has no outgoing segment. Vector keys retain every XYZ value; shared time metadata comes from X. No resampling, inserted keys, normalization of multi-turn angles or expression rewriting occurs. The coupled `value/keys` representation already exists, so there is no project-format change. An already coupled vector is a revision/history-preserving no-op.

The model validates a complete candidate before replacing anything. Mismatched keys or segment interpolation produce a descriptive error and leave the original axes intact. A failed command batch restores project, revision and undo/redo state. Locking, missing targets, unsupported scalar properties and orbit-driven camera position use existing checks. History accounting explicitly tracks the mutated layer/camera instead of falling back to whole-project counting.

Existing axis expressions already support a coupled vector component. Keeping identical key grids and samples preserves their `numKeys`, `key` and `valueAtTime` inputs. Sources, enabled state, targets, profiles and seeds are unchanged, including disabled unfinished expressions.

`capabilities.separate_dimensions.merge` advertises the command, `aligned_keys_shared_interpolation` policy, no resampling, no-op behavior and rejection preservation. The common command catalogue exposes the same command to API/MCP clients.

## Boundaries and issue status

**#262 remains open.** Its property/menu entry is explicitly deferred by the user's instruction to leave visible UI issues alone. Broader symbolic equivalence is also pending: mismatched key grids, an unkeyed static axis beside animated axes, redundant keys and differently encoded equivalent curves are conservatively rejected, even when a mathematical reduction might exist. No approximate conversion or baking option is supplied. No UI, shortcut, theme token, hit target, dependency or third-party asset changes are included.

## Verification

The focused core suite covers all built-in ease modes, progress/velocity curves, overshoot, elastic motion, signed local key times, quarter frames, multi-turn rotation, distinct axis edits, static values, interpolation mismatch, atomic batch rollback with existing redo history, no-op revisions, camera/locking restrictions, explicit child composition routing, gesture cancellation, save/reopen and package import. Expression fixtures cover both whole-vector and axis targets with key metadata and fractional `valueAtTime` queries.

The host test exercises the actual shared session command/state path, capability declaration, timeline projection, undo, persistence and unchanged saved bytes after rejection. The render test compares actual offscreen GPU pixels and serialized frozen render-plan bytes for five timing configurations at six fractional/boundary times, and requires both visible content and changing output to avoid a vacuous equality check. The MCP test exercises discovery, read-only access rejection, stale revision rejection, undo and redo through the desktop engine worker.

Commands run from this worktree with the portable MSVC environment, `CARGO_BUILD_JOBS=1` and `CARGO_TARGET_DIR=E:/Dev/aem/target`:

| Check | Result | Local log |
| --- | --- | --- |
| `cargo test --locked --offline -j1 -p motion-core --test merge_dimensions` | 8 passed | `issue262-core-tests.log` |
| `cargo test --locked --offline -j1 -p motion-host --test merge_dimensions` | 1 passed | `issue262-host-tests.log` |
| `cargo test --locked --offline -j1 -p motion-render --test merge_dimensions` | 1 passed; 30 pairs of GPU captures and frozen plans identical | `issue262-gpu-tests.log` |
| `cargo test --locked --offline -j1 -p motion-model -p motion-core -p motion-desktop` | All 538 runtime tests passed; 2 existing ignored tests. The final core doctest phase failed to locate dependency crates in the shared build directory; a separate doctest rerun is recorded below. | `issue262-regression.log` |
| `cargo test --locked --offline -j1 -p motion-model -p motion-core --doc` | Passed after rebuilding dependencies; both crates contain 0 doctest examples | `issue262-doctests.log` |

The ignored cases are the existing manual large-document history performance measurement and the native audio-output-device test. The desktop suite passed 321 tests, including the new MCP permissions/revision/history test. No runtime test failed. The regression command's overall exit was nonzero because of the dependency lookup error in its final doctest phase; it must not be described as a completely successful command.

The separate doctest rerun exited successfully after rebuilding dependencies, without source changes. In total, 540 distinct runtime tests passed across the regression, host and GPU suites; the focused core suite is already included in the regression count. `git diff --check` passed. Local logs are under `E:/Dev/aem/artifacts/workspace-stall/`.

These checks compile the desktop test harness, not a new distributable desktop executable. This backend-only branch has not been committed, pushed or merged, and no GitHub issue state was changed. The runnable build from the separate workspace-performance worktree is unchanged.
