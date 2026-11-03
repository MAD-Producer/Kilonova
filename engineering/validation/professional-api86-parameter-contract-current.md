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

The compiled-node test adds a local dev-dependency on the existing workspace
`motion-graph` crate; it introduces no new third-party dependency.
No panel, layout, style, third-party asset, project-format migration,
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

## Compiled node parameter integration

The new actual native integration target compiles a graph with Float, Vec3,
Color and Bool public parameters into a real project-owned `.msfx` package.
The host restores its runtime on insertion and fresh-session reopen, without
manual test registry installation. All four parameters reach the rendered
output. The test exercises keys at composition frames 10/20 (local 0/10), a
Vec3 expression, samples at 10/15.5/19.999/20, atomic rejection of fractional
Bool and Bool expressions, batch undo/redo, exact save/load state and output,
and four independently frozen PNG exports after live deletion/session closure.
It independently computes expected values and checks every output pixel, with
RGB error <=1 quantization unit and exact opaque alpha. This is an arithmetic
golden for a spatially constant graph, not a photographic-shot image golden.

| Gate | Result | Interpretation |
| --- | --- | --- |
| Node-runtime v1 | Compile failure: new test borrowed bytes where EffectPackage requires ownership | Fixed the new fixture with an explicit clone; original failure retained |
| Node-runtime v2 | 0 passed, 1 failed: Color constant used nonexistent output port `color` | Fixed the authored connection to the actual `value` port; no product code or assertion changed |
| Node-runtime v3 | 9 passed, 0 failed, 0 ignored; 15.828 seconds | New node GPU test plus existing eight host tests; initial center-pixel oracle |
| Node-runtime v4 | 9 passed, 0 failed, 0 ignored; 15.609 seconds | Adds the independent full-frame oracle, sampled Float/Color assertions, Bool expression rejection and frozen revision checks |
| Node-default v1 | 12 passed, 0 failed, 0 ignored; 20.609 seconds | Same node and host tests plus three existing compiler tests, with FFmpeg disabled |

Both final gates retained unchanged Rust-source hashes. Original logs,
controllers, final test source, actual owned packages, projects and PNGs are in
[`professional-api86-node-runtime-v1.zip`](data/professional-api86-node-runtime-v1.zip)
with a [SHA-256 receipt](data/professional-api86-node-runtime-v1.json). The
[API reference](../host-api/effect-parameter-animation.md#compiled-node-package-example)
documents the executable command and artifact replay. The earlier complete
1058-test backend run predates this test/dev-dependency addition; its original
source coverage is not relabeled as final-head full regression.

PR #599 head `2557ab8a` run `41581573413` has three failed desktop jobs and a
successful dependency audit; Rust/GPU was still running at the last snapshot.
Downloading its Windows log failed with a TLS handshake timeout, so the exact
causes of those three job failures have not been independently read here. This
run also predates the node integration commit. The parent's five known panel
fixture errors remain pending; they are not fixed by adding this test.

## Native animated masks, four mattes and precompose

The actual public-host example in `professional_mattes.rs` now covers source
mask node/opacity animation, hidden matte sources, Alpha/Alpha Inverted/Luma/
Luma Inverted, a subtractive target hole, local clip offsets, fractional frames
and nonmonotonic seeks. An independent CPU pixel-center/coverage oracle checks
every channel with a predeclared maximum error of three Unorm8 units. The final
execution observed at most one unit; nested versus flat output was exact.
The example also checks atomic rejection of a target-only precompose, exact
child model preservation, batch undo/redo, save/reopen and independently frozen
PNGs after deleting the live composition reference and closing the Sessions.
This is a repository-authored procedural geometric shot, not photographed
footage, feather acceptance, external media or other working precisions.

Two renderer defects required functional fixes, separate from the API contract:

- Recreating an empty mask output retained the retired texture's geometry
  fingerprint, allowing a cache hit to skip rasterization. Invalidate the
  fingerprint only when the new allocation commits. A raw GPU regression first
  failed with zero center coverage instead of 255, then passed with unchanged
  assertions. Existing transactional admission and reuse tests also pass.
- Equal-size SDR output reconstructed texel centers from interpolated UVs,
  leaking adjacent red into exactly transparent holes. A retained integer-texel
  fragment pipeline now handles equal extents; scaling keeps bilinear sampling.
  Both pipelines share the original 32-byte uniform. A new exact-pixel test also
  preserves genuine positive subquantum-alpha color. Existing alpha, precision,
  tone mapping, resize and allocation-budget assertions remain unchanged.
  One additional pipeline is created per conversion object; creation performance
  is not benchmarked here.

| Gate | Result and disposition |
| --- | --- |
| Matte-shot v1/v2 | Failed independent golden: RGB contamination in transparent holes; original failures retained |
| Matte-shot v3/v4 | Failed later sample: retained renderer blank, fresh renderer matches golden; isolates stale mask output cache |
| Mask-eviction red v1 | New test compilation failed due to missing HashMap type annotation; no GPU proof in this run |
| Mask-eviction red v2 | New raw GPU test fails before fix: center coverage 0 instead of 255 |
| Matte-shot v5 | Native matte/node/camera/host pass; existing low-alpha test rejects an attempted alpha guard; guard removed |
| Matte-shot v6 | Native matte/node/camera/host pass; existing budget test rejects a second uniform; second uniform removed |
| Matte-shot v5/v6 fixture correction | New strict alpha=0.5 case hits GPU UNORM tie 127/128; use 0.25, away from the tie, without relaxing any assertion or budget |
| Matte-shot v7 | 21 passed, 0 failed, 0 ignored; final two-pipeline/shared-uniform implementation |
| Mask-cache regression v1 | 15 mask reuse/admission plus 35 existing composition/matte/host tests pass |
| Matte-final v1 | Final manifest: all 71 Rust tests pass; Python policy fails from missing local PyYAML and sparse-excluded inventory files; overall controller correctly remains failed |
| CI-policy v2 | Install existing CI pin PyYAML 6.0.3, materialize tracked inventory without changes; 108 tests run, OK with one existing skip |
| Matte-default v1 | FFmpeg disabled: native matte example plus eight existing host tests pass, 0 failed/ignored, 39.453 seconds |

All original failures, controllers, logs, source snapshots, editable projects,
command batches, native/golden/frozen PNGs and comparison sheet are retained in
[`professional-api86-matte-shot-v1.zip`](data/professional-api86-matte-shot-v1.zip)
with a [SHA-256 receipt](data/professional-api86-matte-shot-v1.json). The archive
contains 468 members and was read back to verify each recorded member hash.
Its final source hashes match this increment; the older complete backend run
does not cover these renderer fixes and is not counted as final full regression.

The original mask-eviction red v1 controller copied an inaccurate native-shot
scope description. Its actual command only selected the new mask GPU unit test,
which did not compile; no native-shot coverage is attributed to that run.
Early matte-shot controllers hash Rust only, not WGSL or Cargo manifests. The
matte-final and CI-policy gates instead retain and verify Rust, WGSL, Python,
workflow, manifest and lockfile hashes. Both final gates verify unchanged source.
Reading the retired Android test inventory is a CI policy check, not Android
device/runtime validation.

The raw Rust/GPU log for PR #599 head `0f1c3d40`, run `41583900279`, independently
confirms a CI policy failure before Rust tests: the new internal `motion-graph`
dev-dependency omitted explicit `default-features = false`. This increment fixes
that declaration and passes the original policy suite; it does not change the
policy assertion. That CI run ended in failure and is not final-head proof.

## Public real-PNG relink and frozen source identity

`professional_relink.rs` opens registered Sessions through `Host::create` and
uses the actual public media protocol, rather than directly mutating asset
paths in a model. Two decoded PNG plates have stable layer/asset IDs, clip
offsets and distinct source versions. A separately authored frame oracle checks
all RGBA channels at 5, 7.5, 15, 0, 26 and 10 with a fixed one-unit budget. Actual
maximum error was zero for the initial and all relinked frames.

The test proves unchanged model/revision/disk/pixels throughout isolated staging,
one committed edit for both sources, unchanged layer identities/tracks, actual
new pixels in the retained Renderer, one-step public undo/redo, malformed-source
atomic rejection, two externally missing sources recovered in one edit, and
exact saved/reopened output. Old/new independently frozen PNG plans retain their
captured revisions and exact pixels after live layer deletion and Session
closure. The new version's registered source paths were deleted before export;
the test therefore requires retained source handles, not only paths. It does
not claim that undo can recreate externally deleted file bytes; healthy-source
replacement undo/redo is tested before simulating that deletion.

| Gate | Result |
| --- | --- |
| Relink v1 | New test compile failure: `Layer::solid` requires a borrowed name; fixture reference corrected, no tests counted as executed |
| Relink v2 | 9 passed, 0 failed/ignored, 15.796 seconds; actual native relink and eight existing host integration tests |
| Relink v3 | 9 passed, 0 failed/ignored, 15.281 seconds; adds recorded public requests and replay instructions with unchanged assertions |

Both passing gates run with FFmpeg disabled. Rust/WGSL/Python/workflow/manifest/
lockfile source hashes remain unchanged during execution. No production code,
existing assertion, error budget or ignore annotation changes in this increment.
The test is the complete executable operation script. Retained request JSON must
bind fresh revision/document generation and absolute candidate paths on replay.
All images are repository-authored MIT geometry with no external assets, not
photographic footage or other-precision acceptance.

The immutable
[`professional-api86-relink-v1.zip`](data/professional-api86-relink-v1.zip) and
[SHA-256 receipt](data/professional-api86-relink-v1.json) retain 57 members:
initial/recovered usable projects and assets, actual/CPU-golden/frozen PNGs,
protocol requests, replay instructions, comparison sheet, source snapshots and
original failed/passed logs. Every member hash was verified by reading it back.
The intermediate replacement project records the intentionally deleted sources;
open the initial or recovered project for a complete retained asset set.

PR #599 head `fc0a9ed2`, run `41588997953`, has failed Windows/Linux desktop jobs
while other jobs are still running at this snapshot. Its independently downloaded
Windows log confirms exactly five integer-to-FrameRate test fixture errors:
`panels/audio/tests.rs:343,970`, `panels/auxiliary/render_queue/tests.rs:33`,
`panels/layer_sampling/tests.rs:408`, and `panels/project/tests.rs:14`.
No panel changes are included in this branch; the prior panel-clock scope
question remains unanswered. These logs do not certify the new relink commit,
which requires its own CI.

## Evidence boundaries and remaining work

The parameter-only host example checks model/scene state without requesting a GPU surface or
media decoder. It does not prove pixel equivalence, frozen output equivalence,
or final export. Existing reference regression tests exercise full ID identity,
layer offsets, cycles, missing targets, isolation, timeouts and recovery; they
do not constitute actual professional-shot acceptance.

Actual package fixtures cover Float, Vec2, Vec3, Color, Bool, Enum, Curve and a
non-animatable Bool. The separate node gate proves actual compiled-package/GPU
output for Float, Vec3, Color and Bool. Unimplemented defaults and package availability remain
explicitly separate from advertised implemented animation.

The matrix now documents existing professional API boundaries, and the native
example proves one procedural camera/XYZ/effect output path. The separate native
mask/matte/precompose example now has independent full-frame geometric goldens.
The relink example now proves real PNG decoding, source identity, history,
missing-media recovery and frozen source isolation with independent full-frame
goldens. Issue #86 still requires independently sourced or repository-authored
shot goldens and scripts for roto, green screen/spill, screen replacement and
more complex matte/precompose cases. Broader preview/frozen/export agreement, independent
image quality goldens, other working precisions, complete desktop regression
and final-head cross-platform CI remain unaccepted. The native camera example
is not desktop window capture or acceptance of photographed footage.

PR #597 itself still has unfinished desktop panel clock adoption and UI
acceptance; this API increment does not resolve those base-branch requirements.
Android is retired and excluded from acceptance by the user's scope decision.
