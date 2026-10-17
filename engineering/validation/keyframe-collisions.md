# Keyframe collision validation

This increment addresses the occupied-keyframe item in #132. It is stacked on
`fix/backend-stability-stack` (PR #309, base commit
`b479c457149c3f0801664a065b14716c1be76dfe`), which is stacked on PR #307.
The other model, composition, and storage items in #132 require separate review;
this increment does not close that issue.

## Behavior

- Scalar, vector, separated-axis, effect-parameter, and color transfer curve key
  moves/copies reject occupied destinations before modifying the source.
- Generic and color curve tracks share the vacancy check. Whole separated-axis
  edits retain their existing partial-axis collision validation.
- Existing-source same-frame edits remain no-ops. Missing sources still fail.
- Vacant moves/copies preserve sorted keys and metadata. Value-setting APIs keep
  their existing upsert behavior.
- Errors propagate through the existing core transaction and host command reply.
  No error is converted into success or dropped.

The host contract is documented in `engineering/host-api/keyframe-edits.md`.
The former acceptance test that expected destructive replacement now checks
explicit rejection, complete batch rollback, and preservation of redo history.

## Reproduction

Before changing production code, the new model regression target ran against the
base: **2 passed, 3 failed**. The three collision regressions received `Ok(())`
where rejection was required. The existing same-frame and vacant-edit behavior
passed. The local log is `artifacts/workspace-stall/keyframe-collisions-before.log`.

## Verification

Validation runs on Windows using the repository's portable MSVC environment,
the shared Cargo target, `--locked --offline -j1`, and one test thread.

The selected targets cover model collision atomicity without an engine snapshot,
scalar easing and vector spatial handles, separated axes, negative local effect
frames, command batch rollback, undo/redo, byte-for-byte saved-file preservation,
and the actual host command error reply. Existing curve, spatial-path, effect,
layer-clip, and separated-dimension suites cover successful operations.

The final run passed **74 tests, zero failures, zero ignored**:

```text
cargo test --locked --offline -j1 \
  -p motion-model --test key_collisions \
  -p motion-core --test acceptance --test key_collisions --test curves \
  --test spatial_paths --test separate_dimensions --test effects --test layer_clips \
  --test audio_levels --test layer_masks --test vector_group_keys --test runtime_features \
  -p motion-host --test failure_boundaries -- --test-threads=1
```

Counts: core 66, host 3, model 5. The final log is
`artifacts/workspace-stall/keyframe-collisions-tests.log`. An initial broader run
found a second legacy collision expectation in the layer-clip fixture. That test
now explicitly rejects its occupied local frame, then checks successful edits at
a vacant destination while preserving the original key. The earlier failed run
is retained as `keyframe-collisions-first-run.log`.

Existing vendored Naga warnings and the render library's `dropping_copy_types`
warning remain; no new warning was introduced by these changes.

No user-visible UI, Android code, dependencies, or assets are changed. Desktop
interaction and the full cross-platform suite remain GitHub CI responsibilities;
these targeted checks do not claim full #132 acceptance or power-loss durability.

The 74 Rust checks ran on the original #309 head `2e441409`. The final rebase only
adds the parent's CI dependency-prefetch step and validation notes; no tested Rust
code changed during that rebase. New GitHub CI is still required for this head.
