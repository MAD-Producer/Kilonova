# Issue 86: parameter animation contract increment

Status: implemented and locally verified for the scoped API increment; complete
issue #86 and the 3D route remain open. This branch depends on the rational
project-clock adoption in PR #597. Its diff is reviewed against that branch,
not presented as an independent main-ready project port.

## Delivered

- Derive per-instance parameter animation capabilities from the stored schema.
- Share the expression-admission predicate with the published support query.
- Distinguish numeric interpolation, discrete Hold, color-curve LUT sampling,
  non-animatable values and image/resource bindings.
- Correct the existing false cross-property reference capability declaration;
  the reference evaluator already exists and is unchanged.
- Provide an executable host example with actual command validation, clip
  offsets, fractional sampling, atomic rejection, undo/redo and save/reopen.
- Verify enum key boundaries and locked-layer rejection without misreporting
  the unchanged schema as runtime availability.
- Verify cross-parameter sampling and atomic dependency-cycle rejection.

No panel, layout, style, dependency, third-party asset, project-format migration,
worker timeout/restart or expression execution-budget change is included.

## Actual local execution

All records below are retained in the immutable
[`professional-api86-parameter-contract-v1.zip`](data/professional-api86-parameter-contract-v1.zip)
with a [SHA-256 receipt](data/professional-api86-parameter-contract-v1.json).
Controllers freeze Rust-source hashes throughout each run. Original failures
remain failures; successful later runs have separate identities and scopes.

| Gate | Result | Interpretation |
| --- | --- | --- |
| Initial aggregate v1, host target | 6 passed, 0 failed, 0 ignored | Earlier API increment, before enum/lock/reference additions |
| Initial aggregate v1, core-only expressions compilation | Exit 101; rustc terminated with `0xc0000005 STATUS_ACCESS_VIOLATION` while compiling motion-render | No expression tests executed; aggregate failed; root cause not established |
| Expanded host v2 | Compile failure: two new test calls used the private expressions module | Corrected to the existing public `motion_core::expression_reference_source` export; no assertion weakened |
| Expanded host v3 | 8 passed, 0 failed, 0 ignored | Final host schema and command/persistence tests |
| Combined host/core v4 | 17 passed, 0 failed, 0 ignored | Existing expressions target plus final host target |
| Combined host/core/reference v5 | 25 passed, 0 failed, 0 ignored | Existing references 8, existing expressions 9, host 8; final scoped gate |

The combined Cargo selection reuses a shared dependency feature graph rather
than the separate core-only graph that encountered the compiler crash. No
compiler-crash fix is claimed. Existing expression tests and assertions are
unchanged. The v5 command is documented in the
[API reference](../host-api/effect-parameter-animation.md).

## Evidence boundaries and remaining work

The host example checks model/scene state without requesting a GPU surface or
media decoder. It does not prove pixel equivalence, frozen output equivalence,
or final export. Existing reference regression tests exercise full ID identity,
layer offsets, cycles, missing targets, isolation, timeouts and recovery; they
do not constitute actual professional-shot acceptance.

Actual package fixtures cover Float, Vec2, Color, Bool, Enum, Curve and a
non-animatable Bool. Vec3 has shared schema support but no actual package/GPU
fixture in these runs. Unimplemented defaults and package availability remain
explicitly separate from advertised implemented animation.

Issue #86 still requires the broader professional capability matrix, node
parameter integration proof, masks/mattes/precompose/camera/XYZ API examples,
and independently sourced or repository-authored actual-shot goldens and scripts
for roto, green screen/spill, screen replacement, motion graphics camera,
complex matte/precompose, relink and export. Preview/frozen/export agreement,
complete regression and final-head cross-platform CI remain unaccepted.

PR #597 itself still has unfinished desktop panel clock adoption and UI
acceptance; this API increment does not resolve those base-branch requirements.
Android is retired and excluded from acceptance by the user's scope decision.
