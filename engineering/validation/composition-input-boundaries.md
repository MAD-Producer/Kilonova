# Composition input boundary validation

This increment addresses two model boundary items in #132. It is stacked on
PR #311, `fix/desktop-frame-layout`, at
`397f609cf24fcf39b34e40384c647f00582da53f`. PR #307 is already merged;
the remaining chain is #309, #310, #311, then this increment.

## Reproduction and scope

Before the production change, the new model target passed two tests and failed
two (`artifacts/workspace-stall/composition-boundaries-before.log`):

- A valid 256-composition document accepted a direct `Precompose` operation,
  returned success, and acquired a 257th composition. The editing engine's
  transaction validation rejects this later, but direct model callers have
  already received success and a changed model.
- Calling `Project::audio_voices` with a zero active frame rate panicked in
  integer division, even when the document contained no audio. Ordinary project
  loading already rejects zero frame rates; this is a public model API guard,
  not a claim that a validated imported project can take this path.

`Precompose` now checks the existing composition limit after selection/context
validation and before allocating an ID or changing the model. It uses the same
`resource_limit` error as composition creation. The final available slot remains
usable, and selection/range errors keep their existing precedence.

`audio_voices` now rejects a zero active frame rate before arithmetic, returning
a structured `CompositionError` with code `invalid_frame_rate`, the active
composition ID, and the offending `fps`. Existing valid audio timing arithmetic
is unchanged. No result is replaced by silence or success.

These are specific guards, not a promise that every direct model edit is a
transaction. The engine remains responsible for whole-command-batch rollback.
`CompositionClip::source_frame`, remaining storage durability work, and the
other #132 items are outside this increment. #132 remains open.

## Verification

New tests cover both the main and a nested active view at the composition limit,
successful use of the final slot, unchanged invalid-selection/range precedence,
and zero-rate audio error context without model mutation. The core regression
checks a failed saved batch after a preceding rename: project content, revision,
history accounting, pending redo, and saved bytes remain intact. The host test
exercises command parsing and dispatch, asserting `ok: false` and structured
`error_detail.code: resource_limit` without session changes.

Existing composition, mixed-frame-rate audio, audio level/reference, and
subframe clip suites exercise successful edits and timing behavior, including
24, 25, 29, 59, 90, 144, and 239 fps and repeated precomposition.

```text
cargo test --locked --offline -j1 \
  -p motion-model --test composition_boundaries \
  -p motion-core --test compositions --test composition_resources \
  --test composition_fps --test audio_levels --test audio_references \
  --test subframe_clips \
  -p motion-host --test failure_boundaries -- --test-threads=1
```

The run uses Windows, the repository's portable MSVC environment, and the
agent-owned isolated Cargo target at
`artifacts/workspace-stall/desktop-frame-layout-target`. Debug info and
incremental compilation are disabled to bound disk use. The final log is
`artifacts/workspace-stall/composition-boundaries-tests.log`.
Result: **48 passed, 0 failed, 0 ignored** across eight test targets. Six tests
are new (four model, one core, one host); the other 42 exercise existing
composition and audio behavior. Before the guards, two of the four new model
tests failed: one observed a 257th composition and one a division-by-zero panic.

## Error-handling audit

The current root `AGENTS.md` error-handling rules were reviewed even though this
stack's pinned parent predates their addition. Functional guards and existing
observations are distinguished below. No assertion in an existing test was
weakened, and no logger was added inside model edits or session locks.

| ID | Classification | Evidence and limits |
| --- | --- | --- |
| CB-01 | Functional boundary fixed | The direct-model capacity tests reject total 256 -> 257 before mutation, permit 255 -> 256 in both active views, and preserve invalid-selection precedence. The core saved-batch test additionally checks rollback, revision, redo, history accounting, and saved bytes. |
| CB-02 | Functional boundary fixed | Zero-rate `audio_voices` returns a typed `invalid_frame_rate` error with active composition and fps instead of dividing by zero. Tested with both active views and no audio layers. Valid project loading already rejects this state; this is public API defense. |
| CB-03 | Existing protocol coverage verified | The real host command parser and dispatcher return `ok: false` with structured `resource_limit` details. Core and host callers retain `Result` semantics; no error is replaced with silence or an empty successful value. |
| CB-04 | Existing observation coverage verified | The desktop worker's existing Info start, `respond` Error, and process file/stderr logger are used. The new desktop subprocess test triggers two real over-capacity batches, verifies request/operation/timing/cause and start-before-error order, checks file and Error stderr at default level, stdout isolation, no command-text leak, rollback, cleared in-flight state, and a subsequent successful edit. The executed test passed; evidence is listed below. |
| CB-05 | Normal branches retained | An empty valid audio plan is normal, not a fault. The last composition slot remains available. Best-effort reply semantics, healthy Present behavior, observation thresholds, panic hooks, and worker lifecycle are unchanged. No new counters or logging keys are introduced. |
| CB-06 | Pending, not fixed by documentation | `CompositionClip::source_frame` still needs its own local frame-rate guard and propagation through scene sampling and image prefetch. Remaining #132 graph/storage/path-contract items need separate evidence. No claim that #132 is complete or that invalid fps is reachable through validated ordinary editing. |

The model crate does not own a process logger; these guards return context to
the caller rather than logging inside business-state locks. Desktop command
failures are observed at the existing outer worker reply boundary. The direct
model zero-fps test proves an error return, not a desktop runtime logging path
for a state rejected by project loading.

The additional desktop test passed on Windows (**1 passed, 0 failed**, 361
unrelated tests filtered), bringing this increment's validation to **49 passed**
and seven new tests. Command:

```text
cargo test --locked --offline -j1 -p motion-desktop --bin motion-studio \
  engine::failure::composition_tests::capacity_failure_reaches_reply_file_and_stderr_at_default_level \
  -- --exact
```

The build/test log is `artifacts/workspace-stall/composition-boundaries-desktop.log`.
A direct invocation of the same subprocess case additionally retains the actual
default-level output for inspection:

- `artifacts/workspace-stall/composition-boundary-default-output/motion-studio.log`
- `artifacts/workspace-stall/composition-boundary-default-stderr.log`
- `artifacts/workspace-stall/composition-boundary-default-stdout.log`

These local artifacts are not committed. CI repeats the subprocess assertions;
its test result does not depend on the local artifact paths.

No user-visible UI, Android implementation, dependency, or third-party asset is
changed. The two guard failures propagate through existing `Result` and host
error-envelope paths; the broad error-type refactor in #138 is not included.
