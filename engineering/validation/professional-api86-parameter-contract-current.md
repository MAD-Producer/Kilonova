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

## Native procedural camera output increment

The [professional API matrix and executable camera example](../host-api/professional-compositing-api.md)
document existing mask/matte/precompose/camera/XYZ boundaries and add actual
GPU output evidence for one repository-authored procedural camera shot. The
new integration target uses the real desktop platform and native FFmpeg.

The native-shot v4 gate executed the new GPU/export test and all eight existing
host clock/parameter tests: 9 passed, 0 failed, 0 ignored, source hashes unchanged,
18.265 seconds including compilation. The 320x180, 36-frame, 30000/1001 shot
provides the following evidence:

- The spatial center is coral `[190,24,9,255]`, whereas the same painter-order
  stack with 3D disabled is blue `[13,56,178,255]`.
- Camera/rotation/effect animation changes 19,702 pixels by more than 16 in at
  least one RGB channel between the first and last samples.
- The real API batch undoes in one step, redoes exactly, and survives save/load
  into another Session with identical headless GPU RGBA output.
- Four independently rendered frozen PNGs, including fractional frame 17.5,
  match the original headless samples byte for byte after live edits and closure.
- Every one of the 36 decoded MP4 frames satisfies the predeclared RGB mean
  error budget <=3; the maximum observed frame mean is 1.526424. Individual
  YUV420/codec edge errors reach 164 and are retained, not hidden by the mean.
- All 36 video packets, their exact rational PTS/durations and average frame
  rate satisfy their assertions, as do PNG dimensions, completion and the
  revisions captured by both frozen exporters.

The native evidence archive is
[`professional-api86-native-shot-v1.zip`](data/professional-api86-native-shot-v1.zip)
with a [SHA-256 receipt](data/professional-api86-native-shot-v1.json).
It retains successful native v1/v2/v4 runs and the failed v3 control run. V1's
controller description accidentally retained the earlier parameter gate's prose;
its recorded command and summaries describe the actual two targets. V2 onward
correct that description. V3 exposed an incorrectly arranged new test fixture:
`Add` appends in painter order, so blue was below coral in both modes. V4 adds an
explicit blue-layer reorder before rendering; the original blue-center assertion
is retained. No product behavior or pre-existing assertion changed to pass.

The archive includes the initial/saved editable projects, actual JSON command
batch, later live state, 2D control, all 36 headless movie-reference PNGs, four
headless/frozen pairs and the native MP4. The full selected backend regression
below is separate evidence, not an inference from these nine passing tests.

## Complete selected backend regression

The backend-regression v1 gate completed successfully in 934.156 seconds with
unchanged Rust-source hashes: **1058 passed, 0 failed, 3 existing manual tests
ignored**, across 162 test-result summaries. It ran complete test targets and
doc tests for `motion-model`, `motion-core`, `motion-host` and
`motion-desktop-media`, with default GPU test features and real FFmpeg enabled:

```sh
cargo test -p motion-model -p motion-core -p motion-host -p motion-desktop-media --features motion-desktop-media/ffmpeg --no-fail-fast --locked --offline -- --test-threads=1
```

The three existing ignored tests are the large-document history performance
measurement, automatic keyframe timing GPU/movie proof and frozen keyframe
timing GPU/movie proof. They are not counted as passed in this gate. The two
movie proofs are then explicitly executed separately with unchanged assertions;
their result is recorded below. The history performance measurement is not run.
No ignore annotation is added, removed or changed by this increment.

This is complete selected backend package coverage on the recorded source,
not full workspace/desktop application acceptance. Desktop panel clock adoption,
native window screenshots, final-head platform CI and Android are outside the
scope of this command; Android is retired by the user's explicit instruction.

The separate manual-timing v1 gate then executed both existing movie proofs:
**2 passed, 0 failed, 0 ignored**, source hashes unchanged, 10.391 seconds.
It retained 11 MP4 controls/outputs, 66 PNGs, compiled fixture packages and
comparison receipts in
[`professional-api86-manual-timing-v1.zip`](data/professional-api86-manual-timing-v1.zip)
with a [SHA-256 receipt](data/professional-api86-manual-timing-v1.json).
Its command explicitly requests existing ignored movie tests; it does not
remove their annotations or change their assertions:

```sh
cargo test -p motion-host -p motion-desktop-media --lib --features motion-desktop-media/ffmpeg movie:: --locked --offline -- --ignored --test-threads=1
```

The host library is selected to preserve dependency feature unification and
executes zero matching tests in this manual gate. The remaining ignored history
performance measurement is not represented as an executed or passing test.

PR #599's earlier production/API head `90f45519` completed CI run `41577176324`:
Rust/GPU and dependency audit passed, three desktop jobs failed. That run predates
the native camera test and these expanded evidence records, so it is not proof
for the final follow-up commit. The parent PR #597 run `41574083851` independently
confirmed five integer-to-FrameRate panel test fixture errors; no panel changes
are included here. The final follow-up head still requires its own CI.

## Evidence boundaries and remaining work

The parameter-only host example checks model/scene state without requesting a GPU surface or
media decoder. It does not prove pixel equivalence, frozen output equivalence,
or final export. Existing reference regression tests exercise full ID identity,
layer offsets, cycles, missing targets, isolation, timeouts and recovery; they
do not constitute actual professional-shot acceptance.

Actual package fixtures cover Float, Vec2, Color, Bool, Enum, Curve and a
non-animatable Bool. Vec3 has shared schema support but no actual package/GPU
fixture in these runs. Unimplemented defaults and package availability remain
explicitly separate from advertised implemented animation.

The matrix now documents existing professional API boundaries, and the native
example proves one procedural camera/XYZ/effect output path. Issue #86 still
requires node parameter integration proof, executed shared mask/matte/precompose
examples and independently sourced or repository-authored actual-shot goldens
and scripts for roto, green screen/spill, screen replacement, complex
matte/precompose and relink. Broader preview/frozen/export agreement, independent
image quality goldens, other working precisions, complete desktop regression
and final-head cross-platform CI remain unaccepted. The native camera example
is not desktop window capture or acceptance of photographed footage.

PR #597 itself still has unfinished desktop panel clock adoption and UI
acceptance; this API increment does not resolve those base-branch requirements.
Android is retired and excluded from acceptance by the user's scope decision.
